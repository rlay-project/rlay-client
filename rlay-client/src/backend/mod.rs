#![allow(unused_imports)]
use ::futures::compat::{Compat, Future01CompatExt};
use ::futures::future::{self, BoxFuture, Either, FutureExt, TryFutureExt};
use cid::Cid;
use failure::{err_msg, Error};
use rlay_backend::{BackendFromConfigAndSyncState, BackendRpcMethods};
use rlay_ontology::ontology::Entity;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::config::backend::BackendConfig;

pub use rlay_backend_ethereum::{EthereumBackend, SyncState as EthereumSyncState};
#[cfg(feature = "backend_neo4j")]
pub use rlay_backend_neo4j::{
    config::Neo4jBackendConfig, Neo4jBackend, SyncState as Neo4jSyncState,
};
#[cfg(feature = "backend_redisgraph")]
pub use rlay_backend_redisgraph::{
    config::RedisgraphBackendConfig, RedisgraphBackend, SyncState as RedisgraphSyncState,
};

#[derive(Clone)]
pub enum SyncState {
    Ethereum(EthereumSyncState),
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jSyncState),
    #[cfg(feature = "backend_redisgraph")]
    Redisgraph(RedisgraphSyncState),
}

impl SyncState {
    pub fn new_ethereum() -> Self {
        SyncState::Ethereum(EthereumSyncState::new())
    }

    #[cfg(feature = "backend_neo4j")]
    pub fn new_neo4j_empty(config: &Neo4jBackendConfig) -> Self {
        SyncState::Neo4j(Neo4jSyncState {
            connection_pool: None
        })
    }

    #[cfg(feature = "backend_neo4j")]
    pub async fn new_neo4j(config: &Neo4jBackendConfig) -> Self {
        SyncState::Neo4j(Neo4jSyncState {
            connection_pool: Some(Arc::new(async { config.connection_pool().await }.await)),
        })
    }

    #[cfg(feature = "backend_redisgraph")]
    pub fn new_redisgraph_empty(config: &RedisgraphBackendConfig) -> Self {
        SyncState::Redisgraph(RedisgraphSyncState {
            connection_pool: None,
        })
    }

    #[cfg(feature = "backend_redisgraph")]
    pub async fn new_redisgraph(config: &RedisgraphBackendConfig) -> Self {
        SyncState::Redisgraph(RedisgraphSyncState {
            connection_pool: Some(config.connection_pool().await),
        })
    }

    pub fn as_ethereum(self) -> Option<EthereumSyncState> {
        match self {
            SyncState::Ethereum(sync_state) => Some(sync_state),
            #[cfg(feature = "backend_neo4j")]
            _ => None,
        }
    }

    pub fn as_ethereum_ref(&self) -> Option<&EthereumSyncState> {
        match self {
            SyncState::Ethereum(ref sync_state) => Some(sync_state),
            #[cfg(feature = "backend_neo4j")]
            _ => None,
        }
    }

    #[cfg(feature = "backend_neo4j")]
    pub fn as_neo4j(self) -> Option<Neo4jSyncState> {
        match self {
            SyncState::Neo4j(sync_state) => Some(sync_state),
            _ => None,
        }
    }

    #[cfg(feature = "backend_redisgraph")]
    pub fn as_redisgraph(self) -> Option<RedisgraphSyncState> {
        match self {
            SyncState::Redisgraph(sync_state) => Some(sync_state),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub enum Backend {
    Ethereum(EthereumBackend),
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jBackend),
    #[cfg(feature = "backend_redisgraph")]
    Redisgraph(RedisgraphBackend),
}

impl Backend {
    pub fn get_entities(
        &mut self,
        _cids: Vec<String>,
    ) -> impl Future<Output = Result<Vec<Entity>, Error>> + Send + '_ {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => backend.get_entities(_cids.to_vec()).boxed(),
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => backend.get_entities(_cids.to_vec()).boxed(),
            Backend::Ethereum(_) => future::lazy(|_| unreachable!()).boxed(),
        }
    }
}

impl BackendFromConfigAndSyncState for Backend {
    type C = BackendConfig;
    type S = Option<SyncState>;
    type R = Pin<Box<dyn Future<Output = Result<Self, Error>> + Send>>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R {
        match config {
            BackendConfig::Ethereum(config) => {
                let backend = EthereumBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_ethereum().unwrap(),
                );
                backend.map_ok(|backend| Backend::Ethereum(backend)).boxed()
            }
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => {
                let backend = Neo4jBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_neo4j().unwrap(),
                );
                backend.map_ok(|backend| Backend::Neo4j(backend)).boxed()
            }
            #[cfg(feature = "backend_redisgraph")]
            BackendConfig::Redisgraph(config) => {
                let backend = RedisgraphBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_redisgraph().unwrap(),
                );
                backend
                    .map_ok(|backend| Backend::Redisgraph(backend))
                    .boxed()
            }
        }
    }
}

impl BackendRpcMethods for Backend {
    fn store_entity(
        &mut self,
        entity: &Entity,
        options_object: &Value,
    ) -> BoxFuture<Result<Cid, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => {
                BackendRpcMethods::store_entity(backend, entity, options_object)
            }
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => {
                BackendRpcMethods::store_entity(backend, entity, options_object)
            }
            Backend::Ethereum(backend) => {
                BackendRpcMethods::store_entity(backend, entity, options_object)
            }
        }
    }

    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::get_entity(backend, cid),
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => BackendRpcMethods::get_entity(backend, cid),
            Backend::Ethereum(backend) => BackendRpcMethods::get_entity(backend, cid),
        }
    }

    fn list_cids(&mut self, entity_kind: Option<&str>) -> BoxFuture<Result<Vec<String>, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::list_cids(backend, entity_kind),
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => BackendRpcMethods::list_cids(backend, entity_kind),
            Backend::Ethereum(backend) => BackendRpcMethods::list_cids(backend, entity_kind),
        }
    }

    fn neo4j_query(&mut self, query: &str) -> BoxFuture<Result<Vec<String>, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::neo4j_query(backend, query),
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => BackendRpcMethods::neo4j_query(backend, query),
            Backend::Ethereum(backend) => BackendRpcMethods::neo4j_query(backend, query),
        }
    }
}
