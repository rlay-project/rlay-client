pub mod prelude {
    pub use rlay_backend::{BoxFuture, GetEntity, ResolveEntity};
    pub use rlay_ontology::ontology::Entity;
    pub use serde_json::Value;

    pub use super::FilterContext;
    pub use super::RlayFilter;
}

use ambassador::delegatable_trait;
use async_trait::async_trait;
use rlay_backend::{GetEntity, ResolveEntity};
use rlay_ontology::prelude::*;
use serde_json::Value;
use std::sync::Arc;

pub trait FilterBackend: Send + Sync + GetEntity + ResolveEntity {}

impl<T: Send + Sync + GetEntity + ResolveEntity> FilterBackend for T {}

#[derive(Clone)]
pub struct FilterContext {
    pub backend: Arc<dyn FilterBackend>,
    pub params: Value,
}

#[delegatable_trait]
#[async_trait]
pub trait RlayFilter {
    fn filter_name(&self) -> &'static str;

    async fn filter_entities(&self, ctx: FilterContext, entities: Vec<Entity>) -> Vec<bool>;
}
