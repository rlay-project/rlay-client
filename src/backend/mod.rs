#![allow(unused_imports)]
use cid::Cid;
use failure::{err_msg, Error};
use rlay_ontology::ontology::Entity;
use serde_json::Value;
use web3::futures::future::{self, Future};

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
            _ => None,
        }
    }

    pub fn as_ethereum_ref(&self) -> Option<&EthereumSyncState> {
        match self {
            SyncState::Ethereum(ref sync_state) => Some(sync_state),
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

pub trait BackendFromConfigAndSyncState: Sized {
    type C;
    type S;
    type R: Future<Item = Self, Error = Error> + Send;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R;
}

pub enum Backend {
    Ethereum(EthereumBackend),
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jBackend),
}

impl Backend {
    pub fn get_entities(
        &mut self,
        _cids: &[String],
    ) -> impl Future<Item = Vec<Entity>, Error = Error> + Send {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => backend.get_entities(_cids),
            Backend::Ethereum(_) => unimplemented!(),
        }
    }
}

impl BackendFromConfigAndSyncState for Backend {
    type C = BackendConfig;
    type S = Option<SyncState>;
    type R = Box<Future<Item = Self, Error = Error> + Send>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R {
        match config {
            BackendConfig::Ethereum(config) => {
                let backend = EthereumBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_ethereum().unwrap(),
                );
                Box::new(backend.and_then(|backend| Ok(Backend::Ethereum(backend))))
            }
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => {
                let backend = Neo4jBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_neo4j().unwrap(),
                );
                Box::new(backend.and_then(|backend| Ok(Backend::Neo4j(backend))))
            }
            #[cfg(not(feature = "backend_neo4j"))]
            BackendConfig::Neo4j(_) => Box::new(future::err(err_msg(
                "Support for backend type neo4j not compiled in.",
            ))),
        }
    }
}

pub trait BackendRpcMethods {
    #[allow(unused_variables)]
    fn store_entity(
        &mut self,
        entity: &Entity,
        options_object: &Value,
    ) -> Box<Future<Item = Cid, Error = Error> + Send> {
        Box::new(future::err(err_msg(
            "The requested backend does not support this RPC method.",
        )))
    }

    #[allow(unused_variables)]
    fn get_entity(
        &mut self,
        cid: &str,
    ) -> Box<Future<Item = Option<Entity>, Error = Error> + Send> {
        Box::new(future::err(err_msg(
            "The requested backend does not support this RPC method.",
        )))
    }

    #[allow(unused_variables)]
    fn neo4j_query(
        &mut self,
        query: &str,
    ) -> Box<Future<Item = Vec<String>, Error = Error> + Send> {
        Box::new(future::err(err_msg(
            "The requested backend does not support this RPC method.",
        )))
    }
}

impl BackendRpcMethods for Backend {
    #[allow(unused_variables)]
    fn store_entity(
        &mut self,
        entity: &Entity,
        options_object: &Value,
    ) -> Box<Future<Item = Cid, Error = Error> + Send> {
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
    fn get_entity(
        &mut self,
        cid: &str,
    ) -> Box<Future<Item = Option<Entity>, Error = Error> + Send> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::get_entity(backend, cid),
            Backend::Ethereum(backend) => BackendRpcMethods::get_entity(backend, cid),
        }
    }

    #[allow(unused_variables)]
    fn neo4j_query(
        &mut self,
        query: &str,
    ) -> Box<Future<Item = Vec<String>, Error = Error> + Send> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::neo4j_query(backend, query),
            Backend::Ethereum(backend) => BackendRpcMethods::neo4j_query(backend, query),
        }
    }
}
