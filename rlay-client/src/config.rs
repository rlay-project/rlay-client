use failure::{err_msg, Error};
use rlay_backend::BackendFromConfigAndSyncState;
use std::collections::HashMap;
use std::fs::{self, File};
use std::future::Future;
use std::io::Read;
use std::path::Path;
use toml;

pub use self::backend::BackendConfig;
pub use self::rpc::RpcConfig;
use crate::backend::{Backend, SyncState};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// The path the config was loaded from.
    #[serde(skip_deserializing)]
    pub(crate) config_path: Option<String>,
    #[serde(default = "default_data_path")]
    pub data_path: Option<String>,
    #[serde(default = "default_plugins_path")]
    pub plugins_path: String,
    #[serde(default = "default_rpc_section")]
    pub rpc: RpcConfig,
    #[serde(default)]
    pub backend: Option<BackendConfig>,
    #[serde(default)]
    pub backends: Option<HashMap<String, BackendConfig>>,
}

fn default_data_path() -> Option<String> {
    Some("./rlay_data".to_owned())
}

fn default_plugins_path() -> String {
    "./plugins".to_owned()
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
        let mut config: Config = toml::from_str(&contents)?;
        config.config_path = Some(path.to_str().unwrap().to_owned());
        Ok(config)
    }

    pub fn init_data_dir(&self) -> ::std::io::Result<()> {
        let data_path = self.data_path.as_ref().unwrap();

        fs::create_dir_all(data_path)?;
        fs::create_dir_all(Path::new(data_path).join("epoch_payouts"))?;
        Ok(())
    }

    pub fn get_backend_config(&self) -> Result<&BackendConfig, Error> {
        if let Some(backend_config) = &self.backend {
            return Ok(backend_config);
        }
        warn!("No config value found for \"backend\" key. Trying to use \"backends\" instead.");

        if self.backends.clone().unwrap().len() > 1 {
            Err(format_err!("Multiple backends have been configured. Support for multiple backends has been removed. Please use the \"backend\" config key for a singular backend instead."))
        } else if self.backends.clone().unwrap().len() == 0 {
            Err(err_msg("No backends have been configured."))
        } else {
            warn!("Using backend from \"backends\" config key. Support for multiple backends has been removed. Please use the \"backend\" config key for a singular backend instead.");
            Ok(self.backends.as_ref().unwrap().values().next().unwrap())
        }
    }

    pub fn get_backend_with_syncstate(
        &self,
        sync_state: &SyncState,
    ) -> impl Future<Output = Result<Backend, Error>> {
        let config_for_name: &BackendConfig = self.get_backend_config().unwrap();
        let sync_state_for_name: Option<_> = Some(sync_state);

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
        None
    }

    fn default_network_address() -> String {
        "http://127.0.0.1:8546".to_owned()
    }

    fn default_ws_network_address() -> Option<String> {
        Some("ws://127.0.0.1:8547".to_owned())
    }
}

pub mod backend {
    #[cfg(feature = "backend_neo4j")]
    use rlay_backend_neo4j::config::Neo4jBackendConfig;
    #[cfg(feature = "backend_redisgraph")]
    use rlay_backend_redisgraph::config::RedisgraphBackendConfig;

    #[derive(Debug, Deserialize, Clone)]
    #[serde(tag = "type")]
    pub enum BackendConfig {
        #[serde(rename = "neo4j")]
        #[cfg(feature = "backend_neo4j")]
        Neo4j(Neo4jBackendConfig),
        #[serde(rename = "redisgraph")]
        #[cfg(feature = "backend_redisgraph")]
        Redisgraph(RedisgraphBackendConfig),
    }
}
