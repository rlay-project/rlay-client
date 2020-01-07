use cid::Cid;
use failure::{err_msg, Error};
use futures::future::{err, BoxFuture, FutureExt};
use rlay_ontology::ontology::Entity;
use serde_json::Value;
use std::future::Future;

pub trait BackendFromConfigAndSyncState: Sized {
    type C;
    type S;
    type R: Future<Output = Result<Self, Error>> + Send;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R;
}

pub trait BackendRpcMethods {
    #[allow(unused_variables)]
    fn store_entity(
        &mut self,
        entity: &Entity,
        options_object: &Value,
    ) -> BoxFuture<Result<Cid, Error>> {
        err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
        .boxed()
    }

    #[allow(unused_variables)]
    fn store_entities(
        &mut self,
        entities: &Vec<Entity>,
        options_object: &Value,
    ) -> BoxFuture<Result<Vec<Cid>, Error>> {
        err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
        .boxed()
    }

    #[allow(unused_variables)]
    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
        .boxed()
    }

    #[allow(unused_variables)]
    fn get_entities(&mut self, cids: Vec<String>) -> BoxFuture<Result<Vec<Entity>, Error>> {
        err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
        .boxed()
    }

    #[allow(unused_variables)]
    fn list_cids(&mut self, entity_kind: Option<&str>) -> BoxFuture<Result<Vec<String>, Error>> {
        err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
        .boxed()
    }

    #[allow(unused_variables)]
    fn neo4j_query(&mut self, query: &str) -> BoxFuture<Result<Vec<String>, Error>> {
        err(err_msg(
            "The requested backend does not support this RPC method.",
        ))
        .boxed()
    }
}
