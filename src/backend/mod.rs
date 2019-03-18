#![allow(unused_imports)]
use cid::Cid;
use failure::{err_msg, Error};
use rlay_ontology::ontology::Entity;
use serde_json::Value;

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

pub trait BackendFromConfig: Sized {
    type C;

    fn from_config(config: Self::C) -> Result<Self, Error>;
}

pub trait BackendFromConfigAndSyncState: Sized {
    type C;
    type S;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Result<Self, Error>;
}

pub enum Backend {
    Ethereum(EthereumBackend),
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jBackend),
}

impl Backend {
    pub fn get_entities(&mut self, _cids: &[String]) -> Result<Vec<Entity>, Error> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => backend.get_entities(_cids),
            Backend::Ethereum(_) => unimplemented!(),
        }
    }
}

impl BackendFromConfig for Backend {
    type C = BackendConfig;

    fn from_config(config: Self::C) -> Result<Self, Error> {
        match config {
            BackendConfig::Ethereum(config) => {
                Ok(Backend::Ethereum(EthereumBackend::from_config(config)?))
            }
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => Ok(Backend::Neo4j(Neo4jBackend::from_config(config)?)),
            #[cfg(not(feature = "backend_neo4j"))]
            BackendConfig::Neo4j(_) => {
                Err(err_msg("Support for backend type neo4j not compiled in."))
            }
        }
    }
}

impl BackendFromConfigAndSyncState for Backend {
    type C = BackendConfig;
    type S = Option<SyncState>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Result<Self, Error> {
        match config {
            BackendConfig::Ethereum(config) => Ok(Backend::Ethereum(
                EthereumBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_ethereum().unwrap(),
                )?,
            )),
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => {
                Ok(Backend::Neo4j(Neo4jBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_neo4j().unwrap(),
                )?))
            }
            #[cfg(not(feature = "backend_neo4j"))]
            BackendConfig::Neo4j(_) => {
                Err(err_msg("Support for backend type neo4j not compiled in."))
            }
        }
    }
}

pub trait BackendRpcMethods {
    #[allow(unused_variables)]
    fn store_entity(&mut self, entity: &Entity, options_object: &Value) -> Result<Cid, Error> {
        Err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
    }

    #[allow(unused_variables)]
    fn get_entity(&mut self, cid: &str) -> Result<Option<Entity>, Error> {
        Err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
    }

    #[allow(unused_variables)]
    fn neo4j_query(&mut self, query: &str) -> Result<Vec<String>, Error> {
        Err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
    }
}

impl BackendRpcMethods for Backend {
    #[allow(unused_variables)]
    fn store_entity(&mut self, entity: &Entity, options_object: &Value) -> Result<Cid, Error> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => backend.store_entity(entity, options_object),
            Backend::Ethereum(backend) => backend.store_entity(entity, options_object),
        }
    }

    #[allow(unused_variables)]
    fn get_entity(&mut self, cid: &str) -> Result<Option<Entity>, Error> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => backend.get_entity(cid),
            Backend::Ethereum(backend) => backend.get_entity(cid),
        }
    }

    #[allow(unused_variables)]
    fn neo4j_query(&mut self, query: &str) -> Result<Vec<String>, Error> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => backend.neo4j_query(query),
            Backend::Ethereum(backend) => backend.neo4j_query(query),
        }
    }
}
