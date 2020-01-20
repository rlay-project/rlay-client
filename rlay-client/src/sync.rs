use failure::{err_msg, Error};
use std::collections::HashMap;
use tokio_core;

use crate::backend::SyncState;
use crate::config::{BackendConfig, Config};

#[derive(Clone)]
pub struct MultiBackendSyncState {
    backends: HashMap<String, SyncState>,
}

impl MultiBackendSyncState {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    /// Creates backends without connection pools.
    ///
    /// Required because the connection pool needs to be created by the same reactor
    /// as the RPC.
    pub fn add_backend_empty(&mut self, name: String, config: BackendConfig) {
        match config {
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(_config) => {
                self.backends
                    .insert(name, SyncState::new_neo4j_empty(&_config));
            }
            #[cfg(feature = "backend_redisgraph")]
            BackendConfig::Redisgraph(_config) => {
                self.backends
                    .insert(name, SyncState::new_redisgraph_empty(&_config));
            }
        }
    }

    /// Creates backends with connection pools.
    ///
    /// Required because the connection pool needs to be created by the same reactor
    /// as the RPC.
    pub async fn add_backend_conn(&mut self, name: String, config: BackendConfig) {
        match config {
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(_config) => {
                self.backends
                    .insert(name, SyncState::new_neo4j(&_config).await);
            }
            #[cfg(feature = "backend_redisgraph")]
            BackendConfig::Redisgraph(_config) => {
                self.backends
                    .insert(name, SyncState::new_redisgraph(&_config).await);
            }
        }
    }

    pub fn backend(&self, name: &str) -> Option<SyncState> {
        self.backends.get(name).map(|n| n.to_owned())
    }

    pub fn get_backend(&self, backend_name: Option<&str>) -> Result<&SyncState, Error> {
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
            Some(backend_name) => self
                .backends
                .get(backend_name)
                .ok_or_else(|| format_err!("Unable to find backend for name \"{}\"", backend_name)),
        }
    }
}

pub fn run_sync(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();

    let sync_state = {
        let mut sync_state = MultiBackendSyncState::new();
        for (backend_name, config) in config.backends.iter() {
            sync_state.add_backend_empty(backend_name.clone(), config.clone());
        }

        sync_state
    };

    let rpc_config = config.clone();
    let rpc_sync_state = sync_state.clone();
    ::std::thread::spawn(move || {
        crate::rpc::start_rpc(&rpc_config, rpc_sync_state);
    });

    loop {
        eloop.turn(None);
    }
}
