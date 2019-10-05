use failure::{err_msg, Error};
use futures01::prelude::*;
use futures_timer::Interval;
use log::Level::Debug;
use rlay_backend_ethereum::sync_ontology::{EthOntologySyncer, OntologySyncer};
use rlay_backend_ethereum::sync_proposition_ledger::{
    EthPropositionLedgerSyncer, PropositionLedgerSyncer,
};
use rlay_ontology::ontology::Entity;
use rlay_payout::{
    fill_epoch_payouts, fill_epoch_payouts_cumulative, format_redeem_payout_call,
    load_epoch_payouts, retrieve_epoch_start_block, store_epoch_payouts, submit_epoch_payouts,
    Payout, PayoutEpochs,
};
use rustc_hex::ToHex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_core;
use web3;
use web3::types::U256;

use crate::backend::{EthereumSyncState, SyncState};
use crate::config::{BackendConfig, Config};

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

    /// Creates backends without connection pools.
    ///
    /// Required because the connection pool needs to be created by the same reactor
    /// as the RPC.
    pub fn add_backend_empty(&mut self, name: String, config: BackendConfig) {
        match config {
            BackendConfig::Ethereum(_) => {
                self.backends.insert(name, SyncState::new_ethereum());
            }
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(_config) => {
                self.backends.insert(name, SyncState::new_neo4j_empty(&_config));
            }
            #[cfg(feature = "backend_redisgraph")]
            BackendConfig::Redisgraph(_config) => {
                self.backends
                    .insert(name, SyncState::new_redisgraph_empty(&_config));
            }
        }
    }

    /// Creates backends with connection pools.
    ///
    /// Required because the connection pool needs to be created by the same reactor
    /// as the RPC.
    pub async fn add_backend_conn(&mut self, name: String, config: BackendConfig) {
        match config {
            BackendConfig::Ethereum(_) => {
                self.backends.insert(name, SyncState::new_ethereum());
            }
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(_config) => {
                self.backends.insert(name, SyncState::new_neo4j(&_config).await);
            }
            #[cfg(feature = "backend_redisgraph")]
            BackendConfig::Redisgraph(_config) => {
                self.backends
                    .insert(name, SyncState::new_redisgraph(&_config).await);
            }
        }
    }

    pub fn backend(&self, name: &str) -> Option<SyncState> {
        self.backends.get(name).map(|n| n.to_owned())
    }

    pub fn get_backend(&self, backend_name: Option<&str>) -> Result<&SyncState, Error> {
        match backend_name {
            None => {
                if self.backends.len() > 1 {
                    let backend_names: Vec<_> = self.backends.keys().collect();
                    Err(format_err!("Multiple backends have been configured. Must specify the name of a backend to use. Available backends: {:?}", backend_names))
                } else if self.backends.len() == 0 {
                    Err(err_msg("No backends have been configured."))
                } else {
                    Ok(self.backends.values().next().unwrap())
                }
            }
            Some(backend_name) => self
                .backends
                .get(backend_name)
                .ok_or_else(|| format_err!("Unable to find backend for name \"{}\"", backend_name)),
        }
    }

    // #[cfg_attr(debug_assertions, deprecated(note = "Refactoring to swappable backends"))]
    pub fn default_eth_backend(&self) -> EthereumSyncState {
        self.backend("default_eth").unwrap().as_ethereum().unwrap()
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

/// Spawns a stream onto the provided eventloop that logs stats about the SyncState and the
/// ComputedState at a regular interval.
fn spawn_stats_loop(
    eloop: &tokio_core::reactor::Handle,
    sync_state: EthereumSyncState,
    computed_state: ComputedState,
) {
    if !log_enabled!(Debug) {
        return;
    }
    let counter_stream = Interval::new(Duration::from_secs(5))
        .for_each(move |_| {
            let entity_map = sync_state.entity_map();
            let entity_map_lock = entity_map.lock().unwrap();
            let ledger_lock = sync_state.proposition_ledger.lock().unwrap();
            let payout_epochs = computed_state.payout_epochs_cum.lock().unwrap();
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
                if payouts.len() == 0 {
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
                    let proof_str = format_redeem_payout_call(*epoch, &tree, payout);
                    debug!("Payout for 0x{}: {}", payout.address.to_hex(), proof_str);
                }
            }

            Ok(())
        })
        .map_err(|err| {
            error!("{:?}", err);
            ()
        });
    eloop.spawn(counter_stream);
}

/// Spawns stream to continously submit payout roots, if enabled by config.
fn spawn_payout_root_submission(
    eloop: &tokio_core::reactor::Handle,
    config: Config,
    computed_state: ComputedState,
) {
    if config
        .default_eth_backend_config()
        .unwrap()
        .payout_root_submission_disabled
    {
        trace!("Payout root submission disabled in config.");
        return;
    }

    let submit_payouts_eloop = eloop.clone();
    let submit_payouts = Interval::new(Duration::from_secs(5))
        .map_err(|err| {
            error!("{:?}", err);
            ()
        })
        .for_each(move |_| {
            let web3 = config.web3_with_handle(&submit_payouts_eloop.clone());
            let rlay_token_contract = crate::web3_helpers::rlay_token_contract(&config, &web3);
            submit_epoch_payouts(
                config.clone(),
                computed_state.payout_epochs.clone(),
                computed_state.payout_epochs_cum.clone(),
                rlay_token_contract,
            )
            .map(|_| ())
            .map_err(|err| {
                error!("{:?}", err);
                ()
            })
        })
        .map_err(|err| {
            error!("{:?}", err);
            ()
        });
    eloop.spawn(submit_payouts);
}

pub fn run_sync(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();

    let sync_state = {
        let mut sync_state = MultiBackendSyncState::new();
        for (backend_name, config) in config.backends.iter() {
            sync_state.add_backend_empty(backend_name.clone(), config.clone());
        }

        sync_state
    };
    let computed_state = ComputedState::load_from_files(config.clone());

    {
        // Sync ontology concepts from smart contract to local state
        for (backend_name, sync_state) in sync_state.backends.iter() {
            match config.get_backend_config(Some(backend_name)).unwrap() {
                BackendConfig::Ethereum(config) => {
                    let sync_state = sync_state.as_ethereum_ref().unwrap();
                    let mut syncer = EthOntologySyncer::default();
                    let sync_ontology_fut = syncer
                        .sync_ontology(
                            eloop.handle(),
                            config.clone(),
                            sync_state.entity_map(),
                            sync_state.cid_entity_kind_map(),
                            sync_state.block_entity_map(),
                            sync_state.ontology_last_synced_block(),
                        )
                        .map_err(|err| {
                            error!("Sync ontology: {:?}", err);
                            ()
                        });
                    eloop.handle().spawn(sync_ontology_fut);
                }
                _ => {
                    debug!(
                        "No syncing of ontology implemented for backend type of backend \"{}\"",
                        backend_name
                    );
                }
            }
        }
    }
    {
        // Sync proposition ledger from smart contract to local state
        for (backend_name, sync_state) in sync_state.backends.iter() {
            match config.get_backend_config(Some(backend_name)).unwrap() {
                BackendConfig::Ethereum(config) => {
                    let sync_state = sync_state.as_ethereum_ref().unwrap();
                    let mut syncer = EthPropositionLedgerSyncer::default();
                    let sync_proposition_ledger_fut = syncer
                        .sync_ledger(
                            &eloop.handle(),
                            config.clone(),
                            sync_state.proposition_ledger(),
                            sync_state.proposition_ledger_block_highwatermark(),
                        )
                        .map_err(|err| {
                            error!("Sync ledger: {:?}", err);
                            ()
                        });
                    eloop.handle().spawn(sync_proposition_ledger_fut);
                }
                _ => {
                    debug!(
                        "No syncing of proposition ledger implemented for backend type of backend \"{}\"",
                        backend_name
                    );
                }
            }
        }
    }
    {
        // TODO: make compatible with multiple backends
        for (backend_name, _) in sync_state.backends.iter() {
            if backend_name != "default_eth" {
                debug!("payout calculation is only implemented for default_eth backend right now");
                continue;
            }

            let epoch_length: U256 = config
                .default_eth_backend_config()
                .unwrap()
                .epoch_length
                .into();
            let computed_state_calculate_payouts = computed_state.clone();
            let sync_state_calculate_payouts = sync_state.clone();

            let web3 = config.web3_with_handle(&eloop.handle().clone());
            let rlay_token_contract = crate::web3_helpers::rlay_token_contract(&config, &web3);
            let calculate_payouts_fut = retrieve_epoch_start_block(rlay_token_contract).and_then(
                move |epoch_start_block| {
                    Interval::new(Duration::from_secs(15))
                        .and_then(move |_| Ok(epoch_start_block))
                        .for_each(move |epoch_start_block| {
                            fill_epoch_payouts(
                                epoch_start_block,
                                epoch_length,
                                &sync_state_calculate_payouts
                                    .default_eth_backend()
                                    .proposition_ledger_block_highwatermark(),
                                &sync_state_calculate_payouts
                                    .default_eth_backend()
                                    .proposition_ledger(),
                                &computed_state_calculate_payouts.payout_epochs(),
                                &sync_state_calculate_payouts
                                    .default_eth_backend()
                                    .entity_map(),
                            );
                            fill_epoch_payouts_cumulative(
                                &computed_state_calculate_payouts.payout_epochs(),
                                &computed_state_calculate_payouts.payout_epochs_cum(),
                            );
                            Ok(())
                        })
                        .map_err(|err| {
                            error!("{:?}", err);
                            ()
                        })
                },
            );
            eloop.handle().spawn(calculate_payouts_fut);
        }
    }

    for (backend_name, _) in sync_state.backends.iter() {
        if backend_name != "default_eth" {
            debug!("printing stats loop is only implemented for default_eth backend right now");
            continue;
        }
        spawn_stats_loop(
            &eloop.handle(),
            sync_state.default_eth_backend().clone(),
            computed_state.clone(),
        );
    }

    {
        // Store calculated payouts on disk
        for (backend_name, _) in sync_state.backends.iter() {
            if backend_name != "default_eth" {
                debug!("storing calculated payouts is only implemented for default_eth backend right now");
                continue;
            }
            let computed_state_store = computed_state.clone();
            let store_payouts_config = config.clone();
            let store_payouts = Interval::new(Duration::from_secs(5))
                .map_err(|err| {
                    error!("{:?}", err);
                    ()
                })
                .for_each(move |_| {
                    store_epoch_payouts(
                        store_payouts_config.clone(),
                        computed_state_store.payout_epochs(),
                    );
                    Ok(())
                })
                .map_err(|err| {
                    error!("{:?}", err);
                    ()
                });
            eloop.handle().spawn(store_payouts);
        }
    }
    for (backend_name, _) in sync_state.backends.iter() {
        if backend_name != "default_eth" {
            debug!("submitting payout roots is only implemented for default_eth backend right now");
            continue;
        }
        spawn_payout_root_submission(&eloop.handle(), config.clone(), computed_state.clone());
    }

    let rpc_config = config.clone();
    let rpc_sync_state = sync_state.clone();
    ::std::thread::spawn(move || {
        crate::rpc::start_rpc(&rpc_config, rpc_sync_state);
    });

    loop {
        eloop.turn(None);
    }
}
