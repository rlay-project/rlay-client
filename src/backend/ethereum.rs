#[allow(unused_imports)]
use failure::{err_msg, Error};
use backend::{BackendFromConfig, BackendRpcMethods};
use config::backend::EthereumBackendConfig;

pub struct EthereumBackend {
    pub config: EthereumBackendConfig,
}

impl BackendFromConfig for EthereumBackend {
    type C = EthereumBackendConfig;

    fn from_config(config: Self::C) -> Result<Self, Error> {
        Ok(Self { config })
    }
}

impl BackendRpcMethods for EthereumBackend {}
