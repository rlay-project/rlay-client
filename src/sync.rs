use futures_timer::Interval;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_core;
use web3::futures::{self, prelude::*};
use web3::types::{Filter, Log, U256};
use web3;
use web3::DuplexTransport;
use rustc_hex::ToHex;
use rlay_ontology::ontology::EntityKind;

use config::Config;
use sync_ontology::sync_ontology;
use sync_proposition_ledger::{sync_ledger, PropositionLedger};
use payout::{fill_epoch_payouts, fill_epoch_payouts_cumulative, load_epoch_payouts,
             retrieve_epoch_start_block, store_epoch_payouts, submit_epoch_payouts, Payout,
             PayoutEpochs};

// TODO: possibly contribute to rust-web3
/// Subscribe on a filter, but also get all historic logs that fit the filter
pub fn subscribe_with_history(
    web3: &web3::Web3<impl DuplexTransport>,
    filter: Filter,
) -> impl Stream<Item = Log, Error = web3::Error> {
    let history_future = web3.eth().logs(filter.clone());
    let subscribe_future = web3.eth_subscribe().subscribe_logs(filter);

    let combined_future = history_future
        .join(subscribe_future)
        .into_stream()
        .and_then(|(history, subscribe_stream)| {
            let history_stream = futures::stream::iter_ok(history);

            Ok(Stream::chain(history_stream, subscribe_stream))
        })
        .flatten();

    combined_future
}

#[derive(Clone)]
pub struct SyncState {
    pub entity_map: Arc<Mutex<BTreeMap<Vec<u8>, EntityKind>>>,
    pub cid_entity_kind_map: Arc<Mutex<BTreeMap<Vec<u8>, String>>>,
    pub proposition_ledger: Arc<Mutex<PropositionLedger>>,
    pub proposition_ledger_block_highwatermark: Arc<Mutex<u64>>,
}

impl SyncState {
    pub fn new() -> Self {
        let entity_map: BTreeMap<Vec<u8>, EntityKind> = BTreeMap::new();
        let entity_map_mutex = Arc::new(Mutex::new(entity_map));

        let cid_entity_kind_map: BTreeMap<Vec<u8>, String> = BTreeMap::new();
        let cid_entity_kind_map_mutex = Arc::new(Mutex::new(cid_entity_kind_map));

        let proposition_ledger: PropositionLedger = vec![];
        let proposition_ledger_mutex = Arc::new(Mutex::new(proposition_ledger));

        Self {
            entity_map: entity_map_mutex,
            cid_entity_kind_map: cid_entity_kind_map_mutex,
            proposition_ledger: proposition_ledger_mutex,
            proposition_ledger_block_highwatermark: Arc::new(Mutex::new(0u64)),
        }
    }

    pub fn entity_map(&self) -> Arc<Mutex<BTreeMap<Vec<u8>, EntityKind>>> {
        self.entity_map.clone()
    }

    pub fn cid_entity_kind_map(&self) -> Arc<Mutex<BTreeMap<Vec<u8>, String>>> {
        self.cid_entity_kind_map.clone()
    }

    pub fn proposition_ledger(&self) -> Arc<Mutex<PropositionLedger>> {
        self.proposition_ledger.clone()
    }

    pub fn proposition_ledger_block_highwatermark(&self) -> Arc<Mutex<u64>> {
        self.proposition_ledger_block_highwatermark.clone()
    }
}

#[derive(Clone)]
pub struct ComputedState {
    pub payout_epochs: Arc<Mutex<PayoutEpochs>>,
    /// Cummulative epoch payouts
    pub payout_epochs_cum: Arc<Mutex<PayoutEpochs>>,
}

impl ComputedState {
    pub fn new() -> Self {
        let payout_epochs: PayoutEpochs = HashMap::new();
        let payout_epochs_mutex = Arc::new(Mutex::new(payout_epochs));
        let payout_epochs_cum: PayoutEpochs = HashMap::new();
        let payout_epochs_cum_mutex = Arc::new(Mutex::new(payout_epochs_cum));

        Self {
            payout_epochs: payout_epochs_mutex,
            payout_epochs_cum: payout_epochs_cum_mutex,
        }
    }

    pub fn load_from_files(config: Config) -> Self {
        let mut payout_epochs: PayoutEpochs = HashMap::new();
        // Load state from storage
        load_epoch_payouts(config.clone(), &mut payout_epochs);
        let payout_epochs_mutex = Arc::new(Mutex::new(payout_epochs));

        Self {
            payout_epochs: payout_epochs_mutex,
            ..Self::new()
        }
    }

    pub fn payout_epochs(&self) -> Arc<Mutex<PayoutEpochs>> {
        self.payout_epochs.clone()
    }

    pub fn payout_epochs_cum(&self) -> Arc<Mutex<PayoutEpochs>> {
        self.payout_epochs_cum.clone()
    }
}

pub fn run_sync(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();

    let sync_state = SyncState::new();
    let computed_state = ComputedState::load_from_files(config.clone());

    // Sync ontology concepts from smart contract to local state
    let sync_ontology_fut = sync_ontology(
        eloop.handle(),
        config.clone(),
        sync_state.entity_map(),
        sync_state.cid_entity_kind_map(),
    ).map_err(|err| {
        error!("Sync ontology: {:?}", err);
        ()
    });
    // Sync proposition ledger from smart contract to local state
    let sync_proposition_ledger_fut = sync_ledger(
        eloop.handle(),
        config.clone(),
        sync_state.proposition_ledger(),
        sync_state.proposition_ledger_block_highwatermark(),
    ).map_err(|err| {
        error!("Sync ledger: {:?}", err);
        ()
    });
    // Calculate the payouts based on proposition ledger
    let epoch_length: U256 = config.epoch_length.into();
    let calculate_payouts_fut = retrieve_epoch_start_block(&eloop.handle().clone(), config)
        .and_then(|epoch_start_block| {
            Interval::new(Duration::from_secs(15))
                .and_then(move |_| Ok(epoch_start_block))
                .for_each(|epoch_start_block| {
                    fill_epoch_payouts(
                        epoch_start_block,
                        epoch_length,
                        &sync_state.proposition_ledger_block_highwatermark(),
                        &sync_state.proposition_ledger(),
                        &computed_state.payout_epochs(),
                        &sync_state.entity_map(),
                    );
                    fill_epoch_payouts_cumulative(
                        &computed_state.payout_epochs(),
                        &computed_state.payout_epochs_cum(),
                    );
                    Ok(())
                })
                .map_err(|err| {
                    error!("{:?}", err);
                    ()
                })
        });
    let sync_state_counter = sync_state.clone();
    let computed_state_counter = computed_state.clone();
    // Print some statistics about the local state
    let counter_stream = Interval::new(Duration::from_secs(5))
        .for_each(|_| {
            let entity_map_lock = sync_state_counter.entity_map.lock().unwrap();
            let ledger_lock = sync_state_counter.proposition_ledger.lock().unwrap();
            let payout_epochs = computed_state_counter.payout_epochs_cum.lock().unwrap();
            debug!("Num entities: {}", entity_map_lock.len());
            let mut annotation_count = 0;
            let mut class_count = 0;
            let mut individual_count = 0;
            for entity in entity_map_lock.values() {
                match entity {
                    EntityKind::Annotation(_) => annotation_count += 1,
                    EntityKind::Class(_) => class_count += 1,
                    EntityKind::Individual(_) => individual_count += 1,
                    _ => {}
                }
            }
            debug!("--- Num annotation: {}", annotation_count);
            debug!("--- Num class: {}", class_count);
            debug!("--- Num individual: {}", individual_count);
            debug!("Num propositions: {}", ledger_lock.len());

            for (epoch, payouts) in payout_epochs.iter() {
                trace!("Payouts for epoch {}: {:?}", epoch, payouts);
                if payouts.len() <= 0 {
                    trace!("Not enough payouts to build payout tree");
                    continue;
                }
                let tree = Payout::build_merkle_tree(payouts);
                debug!(
                    "submitPayoutRoot({}, \"0x{}\")",
                    epoch,
                    tree.root().to_hex()
                );
                for payout in payouts {
                    let proof_str = ::payout::format_redeem_payout_call(*epoch, &tree, payout);
                    debug!("Payout for 0x{}: {}", payout.address.to_hex(), proof_str);
                }
            }

            Ok(())
        })
        .map_err(|err| {
            error!("{:?}", err);
            ()
        });

    // Store calculated payouts on disk
    let computed_state_store = computed_state.clone();
    let store_payouts = Interval::new(Duration::from_secs(5))
        .map_err(|err| {
            error!("{:?}", err);
            ()
        })
        .for_each(move |_| {
            store_epoch_payouts(config.clone(), computed_state_store.payout_epochs());
            Ok(())
        })
        .map_err(|err| {
            error!("{:?}", err);
            ()
        });

    // Submit calculated payout roots to smart contract
    let submit_handle = eloop.handle().clone();
    let computed_state_submit = computed_state.clone();
    let submit_payouts = match config.payout_root_submission_disabled {
        true => {
            trace!("Payout root submission disabled in config.");
            futures::future::Either::A(futures::future::empty())
        }
        false => {
            let submit_with_interval = Interval::new(Duration::from_secs(5))
                .map_err(|err| {
                    error!("{:?}", err);
                    ()
                })
                .for_each(move |_| {
                    submit_epoch_payouts(
                        &submit_handle,
                        config.clone(),
                        computed_state_submit.payout_epochs.clone(),
                        computed_state_submit.payout_epochs_cum.clone(),
                    ).map(|_| ())
                        .map_err(|err| {
                            error!("{:?}", err);
                            ()
                        })
                })
                .map_err(|err| {
                    error!("{:?}", err);
                    ()
                });
            futures::future::Either::B(submit_with_interval)
        }
    };

    let rpc_config = config.clone();
    let rpc_sync_state = sync_state.clone();
    ::std::thread::spawn(move || {
        ::rpc::start_rpc(&rpc_config, rpc_sync_state);
    });

    eloop
        .run(sync_ontology_fut.join5(
            sync_proposition_ledger_fut,
            calculate_payouts_fut,
            counter_stream,
            store_payouts.join(submit_payouts),
        ))
        .unwrap();
}
