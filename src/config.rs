use failure::Error;
use rustc_hex::FromHex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tokio_core;
use toml;
use url::Url;
use web3::DuplexTransport;
use web3::types::H160;
use web3;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_network_address")]
    /// Address of the host networks RPC
    pub network_address: Option<String>,
    #[serde(default)]
    pub contract_addresses: HashMap<String, String>,
    #[serde(default = "default_data_path")]
    pub data_path: Option<String>,
    #[serde(default = "default_rpc_section")]
    pub rpc: RpcConfig,
    // TODO: should be taken from smart contract
    #[serde(default = "default_epoch_length")]
    pub epoch_length: u64,
}

fn default_network_address() -> Option<String> {
    Some("ws://localhost:8545".to_owned())
}

fn default_data_path() -> Option<String> {
    Some("./data".to_owned())
}

fn default_epoch_length() -> u64 {
    100
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
        let address_bytes = self.contract_addresses.get(name).expect(&format!(
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
        let network_address: Url = self.network_address.as_ref().unwrap().parse().unwrap();
        let transport = match network_address.scheme() {
            #[cfg(feature = "transport_ws")]
            "ws" => web3::transports::WebSocket::with_event_loop(
                    self.network_address.as_ref().unwrap(),
                    eloop_handle
                ).unwrap()
            ,
            #[cfg(feature = "transport_ipc")]
            "file" => 
                web3::transports::Ipc::with_event_loop(
                    self.network_address.as_ref().unwrap(),
                    eloop_handle,
                ).unwrap()
            ,
            _ => panic!(
                "Only \"file://\" (for IPC) and \"ws://\" addresses are currently supported, and the client has to be compiled with the appropriate flag (transport_ipc or transport_ws)."
            ),
        };

        web3::Web3::new(transport)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcConfig {
    #[serde(default = "default_rpc_disabled")]
    pub disabled: bool,
}

fn default_rpc_disabled() -> bool {
    true
}
