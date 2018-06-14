use failure::Error;
use rustc_hex::FromHex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use toml;
use web3::types::H160;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_network_address")]
    pub network_address: Option<String>,
    #[serde(default)]
    pub contract_addresses: HashMap<String, String>,
}

fn default_network_address() -> Option<String> {
    Some("ws://localhost:8545".to_owned())
}

impl Config {
    pub fn default() -> Config {
        toml::from_str("").unwrap()
    }

    pub fn from_path_opt(path: Option<&str>) -> Result<Config, Error> {
        match path {
            Some(inner_path) => Self::from_path(inner_path),
            None => Ok(Self::default()),
        }
    }

    pub fn from_path(path: &str) -> Result<Config, Error> {
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
