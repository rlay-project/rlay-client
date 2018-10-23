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
use rlay_ontology::ontology::Entity;
use log::Level::Debug;

use config::Config;
use sync_ontology::{BlockEntityMap, EntityMap, EthOntologySyncer, OntologySyncer};
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
pub struct MultiBackendSyncState {
    backends: HashMap<String, SyncState>,
}

impl MultiBackendSyncState {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    pub fn add_backend(&mut self, name: String) {
        self.backends.insert(name, SyncState::new());
    }

    pub fn backend(&self, name: &str) -> Option<SyncState> {
        self.backends.get(name).map(|n| n.to_owned())
    }

    // #[cfg_attr(debug_assertions, deprecated(note = "Refactoring to swappable backends"))]
    pub fn default_eth_backend(&self) -> SyncState {
        self.backend("default_eth").unwrap()
    }
}

#[derive(Clone)]
pub struct SyncState {
    pub ontology: OntologySyncState,
    pub proposition_ledger: Arc<Mutex<PropositionLedger>>,
    pub proposition_ledger_block_highwatermark: Arc<Mutex<u64>>,
}

impl SyncState {
    pub fn new() -> Self {
        let ontology = OntologySyncState::new();

        let proposition_ledger: PropositionLedger = vec![];
        let proposition_ledger_mutex = Arc::new(Mutex::new(proposition_ledger));

        Self {
            ontology,
            proposition_ledger: proposition_ledger_mutex,
            proposition_ledger_block_highwatermark: Arc::new(Mutex::new(0u64)),
        }
    }

    pub fn entity_map(&self) -> Arc<Mutex<EntityMap>> {
        self.ontology.entity_map()
    }

    pub fn block_entity_map(&self) -> Arc<Mutex<BlockEntityMap>> {
        self.ontology.block_entity_map()
    }

    pub fn cid_entity_kind_map(&self) -> Arc<Mutex<BTreeMap<Vec<u8>, String>>> {
        self.ontology.cid_entity_kind_map()
    }

    pub fn proposition_ledger(&self) -> Arc<Mutex<PropositionLedger>> {
        self.proposition_ledger.clone()
    }

    pub fn proposition_ledger_block_highwatermark(&self) -> Arc<Mutex<u64>> {
        self.proposition_ledger_block_highwatermark.clone()
    }

    pub fn ontology_last_synced_block(&self) -> Arc<Mutex<Option<u64>>> {
        self.ontology.last_synced_block()
    }
}

#[derive(Clone)]
pub struct OntologySyncState {
    pub entity_map: Arc<Mutex<EntityMap>>,
    pub block_entity_map: Arc<Mutex<BlockEntityMap>>,
    pub cid_entity_kind_map: Arc<Mutex<BTreeMap<Vec<u8>, String>>>,
    pub last_synced_block: Arc<Mutex<Option<u64>>>,
}

impl OntologySyncState {
    pub fn new() -> Self {
        let entity_map = EntityMap::new();
        let entity_map_mutex = Arc::new(Mutex::new(entity_map));

        let block_entity_map = BlockEntityMap::new();
        let block_entity_map_mutex = Arc::new(Mutex::new(block_entity_map));

        let cid_entity_kind_map: BTreeMap<Vec<u8>, String> = BTreeMap::new();
        let cid_entity_kind_map_mutex = Arc::new(Mutex::new(cid_entity_kind_map));
        Self {
            entity_map: entity_map_mutex,
            block_entity_map: block_entity_map_mutex,
            cid_entity_kind_map: cid_entity_kind_map_mutex,
            last_synced_block: Arc::new(Mutex::new(None)),
        }
    }

    pub fn entity_map(&self) -> Arc<Mutex<EntityMap>> {
        self.entity_map.clone()
    }

    pub fn block_entity_map(&self) -> Arc<Mutex<BlockEntityMap>> {
        self.block_entity_map.clone()
    }

    pub fn cid_entity_kind_map(&self) -> Arc<Mutex<BTreeMap<Vec<u8>, String>>> {
        self.cid_entity_kind_map.clone()
    }

    pub fn last_synced_block(&self) -> Arc<Mutex<Option<u64>>> {
        self.last_synced_block.clone()
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

    let sync_state = {
        let mut sync_state = MultiBackendSyncState::new();
        sync_state.add_backend("default_eth".to_owned());

        sync_state
    };
    let computed_state = ComputedState::load_from_files(config.clone());

    // Sync ontology concepts from smart contract to local state
    let mut syncer = EthOntologySyncer::new();
    let sync_ontology_fut = syncer
        .sync_ontology(
            eloop.handle(),
            config.clone(),
            sync_state.default_eth_backend().entity_map(),
            sync_state.default_eth_backend().cid_entity_kind_map(),
            sync_state.default_eth_backend().block_entity_map(),
            sync_state
                .default_eth_backend()
                .ontology_last_synced_block(),
        )
        .map_err(|err| {
            error!("Sync ontology: {:?}", err);
            ()
        });
    // Sync proposition ledger from smart contract to local state
    let sync_proposition_ledger_fut = sync_ledger(
        eloop.handle(),
        config.clone(),
        sync_state.default_eth_backend().proposition_ledger(),
        sync_state
            .default_eth_backend()
            .proposition_ledger_block_highwatermark(),
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
                        &sync_state
                            .default_eth_backend()
                            .proposition_ledger_block_highwatermark(),
                        &sync_state.default_eth_backend().proposition_ledger(),
                        &computed_state.payout_epochs(),
                        &sync_state.default_eth_backend().entity_map(),
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
    let sync_state_counter = sync_state.default_eth_backend().clone();
    let computed_state_counter = computed_state.clone();
    // Print some statistics about the local state
    let counter_stream = match log_enabled!(Debug) {
        true => {
            let counter_stream = Interval::new(Duration::from_secs(5))
                .for_each(|_| {
                    let entity_map = sync_state_counter.entity_map();
                    let entity_map_lock = entity_map.lock().unwrap();
                    let ledger_lock = sync_state_counter.proposition_ledger.lock().unwrap();
                    let payout_epochs = computed_state_counter.payout_epochs_cum.lock().unwrap();
                    debug!("Num entities: {}", entity_map_lock.len());
                    let mut annotation_count = 0;
                    let mut class_count = 0;
                    let mut individual_count = 0;
                    for entity in entity_map_lock.values() {
                        match entity {
                            Entity::Annotation(_) => annotation_count += 1,
                            Entity::Class(_) => class_count += 1,
                            Entity::Individual(_) => individual_count += 1,
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
                            let proof_str =
                                ::payout::format_redeem_payout_call(*epoch, &tree, payout);
                            debug!("Payout for 0x{}: {}", payout.address.to_hex(), proof_str);
                        }
                    }

                    Ok(())
                })
                .map_err(|err| {
                    error!("{:?}", err);
                    ()
                });
            trace!("Payout root submission disabled in config.");
            futures::future::Either::A(counter_stream)
        }
        false => futures::future::Either::B(futures::future::empty()),
    };

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
