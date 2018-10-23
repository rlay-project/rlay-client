#[allow(unused_imports)]
use failure::{err_msg, Error};

use backend::BackendFromConfig;
use config::backend::Neo4jBackendConfig;

pub struct Neo4jBackend {
    pub config: Neo4jBackendConfig,
}

impl BackendFromConfig for Neo4jBackend {
    type C = Neo4jBackendConfig;

    fn from_config(config: Self::C) -> Result<Self, Error> {
        Ok(Self { config })
    }
}
