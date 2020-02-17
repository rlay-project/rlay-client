#[cfg(feature = "rpc")]
pub mod rpc;

use async_trait::async_trait;
// use futures::future::FutureExt;
// use futures::prelude::*;
use rlay_ontology::ontology::Entity;
use std::collections::HashMap;
use std::future::Future;

pub use failure::Error;
pub use futures::future::BoxFuture;
#[cfg(feature = "rpc")]
pub use rpc::BackendRpcMethods;

#[async_trait]
pub trait GetEntity {
    async fn get_entity(&self, cid: &[u8]) -> Result<Option<Entity>, Error>;
}

// impl<'a> GetEntity<'a> for std::collections::BTreeMap<&[u8], Entity> {
// type F = BoxFuture<'a, Result<Option<Entity>, Error>>;

// fn get_entity(&'a self, cid: &[u8]) -> Self::F {
// future::ready(Ok(self.get(cid).map(|n| n.to_owned()))).boxed()
// }
// }

#[async_trait]
pub trait ResolveEntity {
    async fn resolve_entity(&self, cid: &[u8]) -> Result<HashMap<Vec<u8>, Vec<Entity>>, Error>;
}

pub trait BackendFromConfigAndSyncState: Sized {
    type C;
    type S;
    type R: Future<Output = Result<Self, Error>> + Send;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R;
}
