#![allow(unused_imports)]
use ::futures::compat::Future01CompatExt;
use ::futures::future::{self, BoxFuture, FutureExt, TryFutureExt};
use cid::Cid;
use failure::{err_msg, Error};
use rlay_backend::{BackendFromConfigAndSyncState, BackendRpcMethods};
use rlay_ontology::ontology::Entity;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

use crate::config::backend::BackendConfig;

mod ethereum;
#[cfg(feature = "backend_neo4j")]
mod neo4j;

pub use self::ethereum::{EthereumBackend, SyncState as EthereumSyncState};
#[cfg(feature = "backend_neo4j")]
pub use self::neo4j::{Neo4jBackend, SyncState as Neo4jSyncState};

#[derive(Clone)]
pub enum SyncState {
    Ethereum(EthereumSyncState),
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jSyncState),
}

impl SyncState {
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
}

#[derive(Clone)]
pub enum Backend {
    Ethereum(EthereumBackend),
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jBackend),
}

impl Backend {
    pub fn get_entities(
        &mut self,
        _cids: &[String],
    ) -> impl Future<Output = Result<Vec<Entity>, Error>> + Send {
        #[cfg(feature = "backend_neo4j")]
        match self {
            Backend::Neo4j(backend) => backend.get_entities(_cids),
            Backend::Ethereum(_) => unreachable!(),
        }

        #[cfg(not(feature = "backend_neo4j"))]
        #[allow(unreachable_code)]
        match self {
            Backend::Ethereum(_) => future::lazy(|_| Ok(unreachable!())),
        }
    }
}

impl BackendFromConfigAndSyncState for Backend {
    type C = BackendConfig;
    type S = Option<SyncState>;
    type R = Pin<Box<Future<Output = Result<Self, Error>> + Send>>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R {
        match config {
            BackendConfig::Ethereum(config) => {
                let backend = EthereumBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_ethereum().unwrap(),
                );
                backend
                    .and_then(|backend| future::ok(Backend::Ethereum(backend)))
                    .boxed()
            }
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => {
                let backend = Neo4jBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_neo4j().unwrap(),
                );
                backend
                    .and_then(|backend| future::ok(Backend::Neo4j(backend)).into())
                    .boxed()
            }
            #[cfg(not(feature = "backend_neo4j"))]
            BackendConfig::Neo4j(_) => {
                future::err(err_msg("Support for backend type neo4j not compiled in.")).boxed()
            }
        }
    }
}

impl BackendRpcMethods for Backend {
    #[allow(unused_variables)]
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
            Backend::Ethereum(backend) => {
                BackendRpcMethods::store_entity(backend, entity, options_object)
            }
        }
    }

    #[allow(unused_variables)]
    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::get_entity(backend, cid),
            Backend::Ethereum(backend) => BackendRpcMethods::get_entity(backend, cid),
        }
    }

    #[allow(unused_variables)]
    fn neo4j_query(&mut self, query: &str) -> BoxFuture<Result<Vec<String>, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::neo4j_query(backend, query),
            Backend::Ethereum(backend) => BackendRpcMethods::neo4j_query(backend, query),
        }
    }
}
