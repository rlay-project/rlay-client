use failure::Error;
use rustc_hex::FromHex;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::io::Read;
use toml;
use web3::types::H160;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_network_address")]
    pub network_address: Option<String>,
    #[serde(default)]
    pub contract_addresses: HashMap<String, String>,
    #[serde(default = "default_data_path")]
    pub data_path: Option<String>,
    #[serde(default = "default_rpc_section")]
    pub rpc: RpcConfig,
}

fn default_network_address() -> Option<String> {
    Some("ws://localhost:8545".to_owned())
}

fn default_data_path() -> Option<String> {
    Some("./data".to_owned())
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcConfig {
    #[serde(default = "default_rpc_disabled")]
    pub disabled: bool,
}

fn default_rpc_disabled() -> bool {
    true
}
