pub mod prelude {
    pub use rlay_backend::GetEntity;
    pub use rlay_ontology::ontology::Entity;

    pub use super::FilterContext;
    pub use super::RlayFilter;
}

use ambassador::delegatable_trait;
use rlay_backend::{BoxFuture, Error, GetEntity};
use rlay_ontology::prelude::*;

pub struct FilterContext<'a> {
    pub backend: Box<dyn GetEntity<'a, F = BoxFuture<'a, Result<Option<Entity>, Error>>>>,
}

#[delegatable_trait]
pub trait RlayFilter {
    fn filter_name(&self) -> &'static str;

    fn filter_entity(&self, ctx: &FilterContext, entity: &Entity) -> bool;
}
