#![allow(unused_imports)]
use failure::{err_msg, Error};
use serde_json::Value;
use cid::Cid;
use rlay_ontology::ontology::Entity;

use config::backend::BackendConfig;

mod ethereum;
#[cfg(feature = "backend_neo4j")]
mod neo4j;

use self::ethereum::EthereumBackend;
#[cfg(feature = "backend_neo4j")]
use self::neo4j::Neo4jBackend;

pub trait BackendFromConfig: Sized {
    type C;

    fn from_config(config: Self::C) -> Result<Self, Error>;
}

pub enum Backend {
    Ethereum(EthereumBackend),
    #[cfg(feature = "backend_neo4j")] Neo4j(Neo4jBackend),
}

impl BackendFromConfig for Backend {
    type C = BackendConfig;

    fn from_config(config: Self::C) -> Result<Self, Error> {
        match config {
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => Ok(Backend::Neo4j(Neo4jBackend::from_config(config)?)),
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
            "The requested endpoint does not support this RPC method.",
        ))
    }
}

impl BackendRpcMethods for Backend {
    #[allow(unused_variables)]
    fn store_entity(&mut self, entity: &Entity, options_object: &Value) -> Result<Cid, Error> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => backend.store_entity(entity, options_object),
            Backend::Ethereum(_) => unimplemented!(),
        }
    }
}
