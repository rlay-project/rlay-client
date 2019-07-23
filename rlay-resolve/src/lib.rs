use futures::prelude::*;
use rlay_ontology::ontology::Entity;
use std::future::Future;

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + 'a + Send>>;

pub trait ResolveCid<'a> {
    type F: Future<Output = Option<Entity>>;

    fn resolve<B: AsRef<[u8]>>(&self, cid: B) -> Self::F;
}

impl<'a> ResolveCid<'a> for std::collections::BTreeMap<&[u8], Entity> {
    type F = BoxFuture<'a, Option<Entity>>;

    fn resolve<B: AsRef<[u8]>>(&self, cid: B) -> Self::F {
        future::ready(self.get(cid.as_ref()).map(|n| n.to_owned())).boxed()
    }
}
