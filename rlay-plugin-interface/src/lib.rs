pub mod prelude {
    pub use rlay_backend::{BoxFuture, GetEntity, ResolveEntity};
    pub use rlay_ontology::ontology::Entity;
    pub use serde_json::Value;

    pub use super::FilterContext;
    pub use super::RlayFilter;
}

use ambassador::delegatable_trait;
use rlay_backend::{BoxFuture, Error, GetEntity, ResolveEntity};
use rlay_ontology::prelude::*;
use serde_json::Value;
use std::collections::HashMap;

pub trait FilterBackend<'a>:
    Send
    + Sync
    + GetEntity<'a, F = BoxFuture<'a, Result<Option<Entity>, Error>>>
    + ResolveEntity<'a, F = BoxFuture<'a, Result<HashMap<Vec<u8>, Vec<Entity>>, Error>>>
{
}

impl<
        'a,
        T: Send
            + Sync
            + GetEntity<'a, F = BoxFuture<'a, Result<Option<Entity>, Error>>>
            + ResolveEntity<'a, F = BoxFuture<'a, Result<HashMap<Vec<u8>, Vec<Entity>>, Error>>>,
    > FilterBackend<'a> for T
{
}

pub struct FilterContext<'a> {
    pub backend: Box<dyn FilterBackend<'a>>,
    pub params: &'a Value,
}

#[delegatable_trait]
pub trait RlayFilter {
    fn filter_name(&self) -> &'static str;

    fn filter_entities(&self, ctx: &FilterContext, entities: Vec<Entity>) -> BoxFuture<Vec<bool>>;
}
