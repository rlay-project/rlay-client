#[allow(unused_imports)]
use failure::{err_msg, Error};
use config::backend::BackendConfig;

#[cfg(feature = "backend_neo4j")]
mod neo4j;

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

pub struct EthereumBackend {}
