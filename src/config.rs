use failure::{err_msg, Error};
use rustc_hex::FromHex;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use tokio_core;
use toml;
use url::Url;
use web3::DuplexTransport;
use web3::types::H160;
use web3;

use sync::MultiBackendSyncState;
use backend::{Backend, BackendFromConfig, BackendFromConfigAndSyncState};
pub use self::rpc::RpcConfig;
pub use self::backend::{BackendConfig, EthereumBackendConfig, Neo4jBackendConfig};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_data_path")]
    pub data_path: Option<String>,
    #[serde(default = "default_rpc_section")]
    pub rpc: RpcConfig,
    #[serde(default)]
    pub backends: HashMap<String, BackendConfig>,
}

fn default_data_path() -> Option<String> {
    Some("./rlay_data".to_owned())
}

fn default_rpc_section() -> RpcConfig {
    toml::from_str("").unwrap()
}

impl Config {
    pub fn default() -> Config {
        toml::from_str("").unwrap()
    }

    pub fn from_path_opt(path: Option<&str>) -> Result<Config, Error> {
        match path {
            Some(inner_path) => Self::from_path(Path::new(inner_path)),
            None => {
                let default_path = Path::new("rlay.config.toml");
                debug!(
                    "No config file path provided. Looking at default path \"{}\"",
                    default_path.to_string_lossy()
                );
                if default_path.is_file() {
                    Self::from_path(Path::new(default_path))
                } else {
                    debug!("No config file found. Using builtin default config.");
                    Ok(Self::default())
                }
            }
        }
    }

    pub fn from_path(path: &Path) -> Result<Config, Error> {
        debug!(
            "Loading config file from path \"{}\"",
            path.to_string_lossy()
        );
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn contract_address(&self, name: &str) -> H160 {
        let address_bytes = self.default_eth_backend_config()
            .unwrap()
            .contract_addresses
            .get(name)
            .expect(&format!(
                "Could not find configuration key for contract_addresses.{}",
                name
            ))[2..]
            .from_hex()
            .unwrap();

        H160::from_slice(&address_bytes)
    }

    pub fn web3_with_handle(
        &self,
        eloop_handle: &tokio_core::reactor::Handle,
    ) -> web3::Web3<impl DuplexTransport> {
        let network_address: Url = self.default_eth_backend_config()
            .unwrap()
            .network_address
            .as_ref()
            .unwrap()
            .parse()
            .unwrap();
        let transport = match network_address.scheme() {
            #[cfg(feature = "transport_ws")]
            "ws" => web3::transports::WebSocket::with_event_loop(
                    self
                        .default_eth_backend_config()
                        .unwrap()
                        .network_address
                        .as_ref()
                        .unwrap(),
                    eloop_handle
                ).unwrap()
            ,
            #[cfg(feature = "transport_ipc")]
            "file" => 
                web3::transports::Ipc::with_event_loop(
                    network_address.path(),
                    eloop_handle,
                ).unwrap()
            ,
            _ => panic!(
                "Only \"file://\" (for IPC) and \"ws://\" addresses are currently supported, and the client has to be compiled with the appropriate flag (transport_ipc or transport_ws)."
            ),
        };

        web3::Web3::new(transport)
    }

    pub fn init_data_dir(&self) -> ::std::io::Result<()> {
        let data_path = self.data_path.as_ref().unwrap();

        fs::create_dir_all(data_path)?;
        fs::create_dir_all(Path::new(data_path).join("epoch_payouts"))?;
        Ok(())
    }

    // #[cfg_attr(debug_assertions, deprecated(note = "Refactoring to swappable backends"))]
    pub fn default_eth_backend_config(&self) -> Result<&EthereumBackendConfig, Error> {
        let config = self.get_backend_config(Some("default_eth"))?;
        Ok(config.as_ethereum().unwrap())
    }

    pub fn get_backend_config(&self, backend_name: Option<&str>) -> Result<&BackendConfig, Error> {
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
            Some(backend_name) => self.backends.get(backend_name).ok_or(format_err!(
                "Unable to find backend for name \"{}\"",
                backend_name
            )),
        }
    }

    pub fn get_backend(&self, backend_name: Option<&str>) -> Result<Backend, Error> {
        let config_for_name: &BackendConfig = self.get_backend_config(backend_name)?;

        Backend::from_config(config_for_name.to_owned())
    }

    pub fn get_backend_with_syncstate(
        &self,
        backend_name: Option<&str>,
        sync_state: &MultiBackendSyncState,
    ) -> Result<Backend, Error> {
        let config_for_name: &BackendConfig = self.get_backend_config(backend_name)?;
        let sync_state_for_name: Option<_> = sync_state.get_backend(backend_name).ok();

        Backend::from_config_and_syncstate(
            config_for_name.to_owned(),
            sync_state_for_name.map(|n| n.to_owned()),
        )
    }
}

pub mod rpc {
    #[derive(Debug, Deserialize, Clone)]
    pub struct RpcConfig {
        #[serde(default = "default_rpc_disabled")]
        pub disabled: bool,
        #[serde(default = "default_proxy_target_network_address")]
        /// Network address of the upstream Ethereum RPC.
        pub proxy_target_network_address: Option<String>,
        #[serde(default = "default_network_address")]
        /// Network address to serve the RPC on.
        pub network_address: String,
        #[serde(default = "default_ws_network_address")]
        /// Network address to serve the Websocket RPC on.
        pub ws_network_address: Option<String>,
    }

    fn default_rpc_disabled() -> bool {
        true
    }

    fn default_proxy_target_network_address() -> Option<String> {
        Some("http://localhost:8545".to_owned())
    }

    fn default_network_address() -> String {
        "http://127.0.0.1:8546".to_owned()
    }

    fn default_ws_network_address() -> Option<String> {
        Some("ws://127.0.0.1:8547".to_owned())
    }
}

pub mod backend {
    use rustc_hex::FromHex;
    use std::collections::HashMap;
    use web3::types::H160;
    #[cfg(feature = "backend_neo4j")]
    use rusted_cypher::GraphClient;

    #[derive(Debug, Deserialize, Clone)]
    #[serde(tag = "type")]
    pub enum BackendConfig {
        #[serde(rename = "ethereum")] Ethereum(EthereumBackendConfig),
        #[serde(rename = "neo4j")] Neo4j(Neo4jBackendConfig),
    }

    impl BackendConfig {
        pub fn as_ethereum(&self) -> Option<&EthereumBackendConfig> {
            match self {
                BackendConfig::Ethereum(config) => Some(config),
                _ => None,
            }
        }
    }

    #[derive(Debug, Deserialize, Clone)]
    pub struct EthereumBackendConfig {
        #[serde(default = "default_network_address")]
        /// Address of the host networks RPC
        pub network_address: Option<String>,
        #[serde(default)]
        pub contract_addresses: HashMap<String, String>,
        // TODO: should be taken from smart contract
        #[serde(default = "default_epoch_length")]
        pub epoch_length: u64,
        #[serde(default = "default_payout_root_submission_disabled")]
        pub payout_root_submission_disabled: bool,
    }

    fn default_network_address() -> Option<String> {
        Some("ws://localhost:8545".to_owned())
    }

    fn default_epoch_length() -> u64 {
        100
    }

    fn default_payout_root_submission_disabled() -> bool {
        false
    }

    impl EthereumBackendConfig {
        pub fn contract_address(&self, name: &str) -> H160 {
            let address_bytes = self.contract_addresses.get(name).expect(&format!(
                "Could not find configuration key for contract_addresses.{}",
                name
            ))[2..]
                .from_hex()
                .unwrap();

            H160::from_slice(&address_bytes)
        }
    }

    #[derive(Debug, Deserialize, Clone)]
    pub struct Neo4jBackendConfig {
        pub uri: String,
    }

    impl Neo4jBackendConfig {
        #[cfg(feature = "backend_neo4j")]
        pub fn client(&self) -> Result<GraphClient, ::rusted_cypher::error::GraphError> {
            GraphClient::connect(&self.uri)
        }
    }
}
