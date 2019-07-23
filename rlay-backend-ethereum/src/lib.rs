extern crate failure;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

pub mod config;
pub mod data;
pub mod deploy;
pub mod doctor;
pub mod sync_ontology;
pub mod sync_proposition_ledger;
mod web3_helpers;

use failure::Error;
use futures::future::BoxFuture;
use futures::prelude::*;
use rlay_backend::{BackendFromConfigAndSyncState, BackendRpcMethods};
use rlay_ontology::ontology::Entity;
use rustc_hex::FromHex;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::config::EthereumBackendConfig;
use crate::sync_ontology::{BlockEntityMap, EntityMap};
use crate::sync_proposition_ledger::PropositionLedger;

#[derive(Clone)]
pub struct EthereumBackend {
    pub config: EthereumBackendConfig,
    pub sync_state: SyncState,
}

impl BackendFromConfigAndSyncState for EthereumBackend {
    type C = EthereumBackendConfig;
    type S = SyncState;
    type R = Pin<Box<dyn Future<Output = Result<Self, Error>> + Send>>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R {
        future::ok(Self { config, sync_state }).boxed()
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
        let entity_map = EntityMap::default();
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

impl BackendRpcMethods for EthereumBackend {
    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        let entity_map = self.sync_state.entity_map();
        let entity_map_lock = entity_map.lock().unwrap();

        let cid_no_prefix = str::replace(cid, "0x", "");
        let cid_bytes = cid_no_prefix.from_hex().unwrap();

        future::ok(entity_map_lock.get(&cid_bytes).map(|n| n.clone())).boxed()
    }
}
