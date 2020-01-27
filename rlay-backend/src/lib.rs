#[cfg(feature = "rpc")]
pub mod rpc;

use futures::future::FutureExt;
use futures::prelude::*;
use rlay_ontology::ontology::Entity;
use std::future::Future;

pub use failure::Error;
pub use futures::future::BoxFuture;
#[cfg(feature = "rpc")]
pub use rpc::BackendRpcMethods;

pub trait GetEntity<'a> {
    type F: Future<Output = Result<Option<Entity>, Error>>;

    fn get_entity<B: AsRef<[u8]>>(&'a self, cid: B) -> Self::F;
}

impl<'a> GetEntity<'a> for std::collections::BTreeMap<&[u8], Entity> {
    type F = BoxFuture<'a, Result<Option<Entity>, Error>>;

    fn get_entity<B: AsRef<[u8]>>(&'a self, cid: B) -> Self::F {
        future::ready(Ok(self.get(cid.as_ref()).map(|n| n.to_owned()))).boxed()
    }
}

pub trait BackendFromConfigAndSyncState: Sized {
    type C;
    type S;
    type R: Future<Output = Result<Self, Error>> + Send;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R;
}
