use ambassador::Delegate;
use async_trait::async_trait;
use cid::Cid;
use failure::Error;
use futures::future::{BoxFuture, FutureExt, TryFutureExt};
use rlay_backend::rpc::*;
use rlay_backend::{BackendFromConfigAndSyncState, GetEntity, ResolveEntity};
use rlay_ontology::ontology::Entity;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::config::backend::BackendConfig;

#[cfg(feature = "backend_neo4j")]
pub use rlay_backend_neo4j::{
    config::Neo4jBackendConfig, Neo4jBackend, SyncState as Neo4jSyncState,
};
#[cfg(feature = "backend_redisgraph")]
pub use rlay_backend_redisgraph::{
    config::RedisgraphBackendConfig, RedisgraphBackend, SyncState as RedisgraphSyncState,
};

#[derive(Clone)]
pub enum SyncState {
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jSyncState),
    #[cfg(feature = "backend_redisgraph")]
    Redisgraph(RedisgraphSyncState),
}

impl SyncState {
    pub async fn new(config: &BackendConfig) -> Self {
        match config {
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => SyncState::new_neo4j(&config).await,
            #[cfg(feature = "backend_redisgraph")]
            BackendConfig::Redisgraph(config) => SyncState::new_redisgraph(&config).await,
        }
    }

    #[cfg(feature = "backend_neo4j")]
    pub async fn new_neo4j(config: &Neo4jBackendConfig) -> Self {
        SyncState::Neo4j(Neo4jSyncState {
            connection_pool: Some(async { config.connection_pool().await }.await),
        })
    }

    #[cfg(feature = "backend_redisgraph")]
    pub async fn new_redisgraph(config: &RedisgraphBackendConfig) -> Self {
        SyncState::Redisgraph(RedisgraphSyncState {
            connection_pool: Some(config.connection_pool().await),
        })
    }

    #[cfg(feature = "backend_neo4j")]
    pub fn as_neo4j(self) -> Option<Neo4jSyncState> {
        match self {
            SyncState::Neo4j(sync_state) => Some(sync_state),
            _ => None,
        }
    }

    #[cfg(feature = "backend_redisgraph")]
    pub fn as_redisgraph(self) -> Option<RedisgraphSyncState> {
        match self {
            SyncState::Redisgraph(sync_state) => Some(sync_state),
            _ => None,
        }
    }
}

sa::assert_impl_all!(Backend: Send);
#[derive(Clone, Delegate)]
#[delegate(rlay_backend::BackendRpcMethodGetEntity)]
// TODO: Bugged; See https://github.com/hobofan/ambassador/issues/16
// #[delegate(rlay_backend::BackendRpcMethodGetEntities)]
// #[delegate(rlay_backend::BackendRpcMethodResolveEntities)]
#[delegate(rlay_backend::BackendRpcMethodStoreEntity)]
#[delegate(rlay_backend::BackendRpcMethodStoreEntities)]
#[delegate(rlay_backend::BackendRpcMethodResolveEntity)]
#[delegate(rlay_backend::BackendRpcMethodListCids)]
#[delegate(rlay_backend::BackendRpcMethodNeo4jQuery)]
pub enum Backend {
    #[cfg(feature = "backend_neo4j")]
    Neo4j(Neo4jBackend),
    #[cfg(feature = "backend_redisgraph")]
    Redisgraph(RedisgraphBackend),
}

impl BackendFromConfigAndSyncState for Backend {
    type C = BackendConfig;
    type S = Option<SyncState>;
    type R = Pin<Box<dyn Future<Output = Result<Self, Error>> + Send>>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R {
        match config {
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(config) => {
                let backend = Neo4jBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_neo4j().unwrap(),
                );
                backend.map_ok(|backend| Backend::Neo4j(backend)).boxed()
            }
            #[cfg(feature = "backend_redisgraph")]
            BackendConfig::Redisgraph(config) => {
                let backend = RedisgraphBackend::from_config_and_syncstate(
                    config,
                    sync_state.unwrap().as_redisgraph().unwrap(),
                );
                backend
                    .map_ok(|backend| Backend::Redisgraph(backend))
                    .boxed()
            }
        }
    }
}

impl BackendRpcMethodGetEntities for Backend {
    fn get_entities(&mut self, cids: Vec<String>) -> BoxFuture<Result<Vec<Entity>, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::get_entities(backend, cids),
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => BackendRpcMethods::get_entities(backend, cids),
        }
    }
}

impl BackendRpcMethodResolveEntities for Backend {
    fn resolve_entities(
        &mut self,
        cids: Vec<String>,
    ) -> BoxFuture<Result<HashMap<String, Vec<Entity>>, Error>> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => BackendRpcMethods::resolve_entities(backend, cids),
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => BackendRpcMethods::resolve_entities(backend, cids),
        }
    }
}

impl BackendRpcMethods for Backend {}

#[async_trait]
impl GetEntity for Backend {
    async fn get_entity(&self, cid: &[u8]) -> Result<Option<Entity>, Error> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => GetEntity::get_entity(backend, cid).await,
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => GetEntity::get_entity(backend, cid).await,
        }
    }
}

#[async_trait]
impl ResolveEntity for Backend {
    async fn resolve_entity(&self, cid: &[u8]) -> Result<HashMap<Vec<u8>, Vec<Entity>>, Error> {
        match self {
            #[cfg(feature = "backend_neo4j")]
            Backend::Neo4j(backend) => ResolveEntity::resolve_entity(backend, cid).await,
            #[cfg(feature = "backend_redisgraph")]
            Backend::Redisgraph(backend) => ResolveEntity::resolve_entity(backend, cid).await,
        }
    }
}
