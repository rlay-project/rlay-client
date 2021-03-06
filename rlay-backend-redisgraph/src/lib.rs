#![recursion_limit = "128"]

#[macro_use]
extern crate log;
extern crate failure;
#[macro_use]
extern crate serde_derive;
extern crate static_assertions as sa;

pub mod config;
mod parse;

use async_trait::async_trait;
use cid::{Cid, ToCid};
use failure::{format_err, Error};
use futures::future::BoxFuture;
use futures::prelude::*;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use redis::{aio::MultiplexedConnection, FromRedisValue};
use rlay_backend::rpc::*;
use rlay_backend::{BackendFromConfigAndSyncState, GetEntity, ResolveEntity};
use rlay_ontology::prelude::*;
use rustc_hex::ToHex;
use serde_json::Value;
use std::collections::HashMap;

use crate::config::RedisgraphBackendConfig;
use crate::parse::{CidList, GetQueryRelationship};

sa::assert_impl_all!(RedisgraphBackend: Send, Sync);
#[derive(Clone)]
pub struct RedisgraphBackend {
    pub config: RedisgraphBackendConfig,
    client: OnceCell<MultiplexedConnection>,
}

impl RedisgraphBackend {
    pub fn from_config(config: RedisgraphBackendConfig) -> Self {
        Self {
            config,
            client: OnceCell::new(),
        }
    }

    pub async fn client(&self) -> Result<MultiplexedConnection, Error> {
        if let Some(client) = self.client.get() {
            return Ok(client.clone());
        }

        trace!("Creating new connection pool for backend.");
        let new_connection = self.config.connection_pool().await;
        let _ = self.client.set(new_connection.clone());
        Ok(new_connection)
    }

    async fn get_entity(&self, cid: String) -> Result<Option<Entity>, Error> {
        let mut client = self.client().await?;

        let query = format!(
            "MATCH (n:RlayEntity {{ cid: '{0}' }})-[r]->(m) RETURN n,type(r),m",
            cid
        );
        trace!("get_entity query: {:?}", query);

        let query_res: Option<redis::Value> = redis::cmd("GRAPH.QUERY")
            .arg(&self.config.graph_name)
            .arg(query)
            .query_async(&mut client)
            .await
            // TODO: cast error to none; Required because missing graph is throwing an error
            .ok();
        if let None = query_res {
            return Ok(None);
        }
        let query_res = query_res.unwrap();
        let results_with_meta = Vec::<redis::Value>::from_redis_value(&query_res).unwrap();
        let results = Vec::<redis::Value>::from_redis_value(&results_with_meta[1]).unwrap();

        let relationships: Vec<GetQueryRelationship> = results
            .into_iter()
            .map(|n| GetQueryRelationship::parse(n))
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
            .unwrap();
        let entity = GetQueryRelationship::merge_into_entity(relationships).unwrap();
        if let None = entity {
            return Ok(None);
        }
        let entity = entity.unwrap();

        let retrieved_cid = format!("0x{}", entity.to_cid().unwrap().to_bytes().to_hex());
        if retrieved_cid != cid {
            return Err(format_err!(
                "The retrieved CID did not match the requested cid: {} !+ {}",
                cid,
                retrieved_cid
            ));
        }

        Ok(Some(entity))
    }

    pub async fn get_entities(&mut self, cids: Vec<String>) -> Result<Vec<Entity>, Error> {
        let cids: Vec<String> = cids.to_owned();
        let mut client = self.client().await?;

        let deduped_cids = {
            let mut deduped_cids = cids.to_owned();
            deduped_cids.dedup();
            deduped_cids
        };

        let query = format!(
            "MATCH (n:RlayEntity)-[r]->(m) WHERE n.cid IN {0:?} RETURN n,type(r),m",
            deduped_cids,
        );
        trace!("get_entities query: \"{}\"", query);

        let query_res: Option<redis::Value> = redis::cmd("GRAPH.QUERY")
            .arg(&self.config.graph_name)
            .arg(query)
            .query_async(&mut client)
            .await
            // TODO: cast error to none; Required because missing graph is throwing an error
            .ok();
        if let None = query_res {
            return Ok(vec![]);
        }
        let query_res = query_res.unwrap();
        let results_with_meta = Vec::<redis::Value>::from_redis_value(&query_res).unwrap();
        let results = Vec::<redis::Value>::from_redis_value(&results_with_meta[1]).unwrap();

        let relationships: Vec<GetQueryRelationship> = results
            .into_iter()
            .map(|n| GetQueryRelationship::parse(n))
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
            .unwrap();
        let entities: Vec<Entity> = relationships
            .into_iter()
            .group_by(|n| n.n_id)
            .into_iter()
            .filter_map(|(_, group)| {
                GetQueryRelationship::merge_into_entity(group.into_iter().collect()).unwrap()
            })
            .collect();

        trace!("get_entities retrieved {} entities", entities.len());
        debug_assert!(
            deduped_cids.len() == entities.len(),
            "{} cids provided and {} entities retrieved",
            deduped_cids.len(),
            entities.len()
        );

        Ok(entities)
    }

    async fn query_entities(&mut self, query: String) -> Result<Vec<String>, Error> {
        let mut client = self.client().await?;

        dbg!(&query);
        let query_res: Option<redis::Value> = redis::cmd("GRAPH.QUERY")
            .arg(&self.config.graph_name)
            .arg(query)
            .query_async(&mut client)
            .await
            .ok();
        dbg!(&query_res);
        if let None = query_res {
            return Ok(vec![]);
        }
        let query_res = query_res.unwrap();
        let results_with_meta = Vec::<redis::Value>::from_redis_value(&query_res).unwrap();
        if results_with_meta.len() < 2 {
            return Ok(vec![]);
        }

        let parsed = CidList::parse(results_with_meta[1].clone()).unwrap();
        Ok(parsed.inner)
    }

    async fn store_entity(&mut self, entity: Entity) -> Result<Cid, Error> {
        let raw_cid = entity.to_cid().unwrap();
        let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());

        let kind_name: &str = entity.kind().into();
        let entity_val = serde_json::to_value(FormatWeb3(entity.clone())).unwrap();
        let val = entity_val.as_object().unwrap();

        let mut values = Vec::new();
        let mut relationship_queries = Vec::new();
        {
            let mut add_relationship_value = |source_cid, key, target_value| {
                {
                    let rel_query = format!(
                        "MERGE (m:RlayEntity {{ cid: '{target_value}' }})",
                        target_value = target_value
                    );
                    relationship_queries.push(rel_query);
                }
                {
                    let rel_query = format!(
                        "MATCH (n:RlayEntity {{ cid: '{source_cid}'}}),(m:RlayEntity {{ cid: '{target_value}' }}) CREATE (n)-[r:{relationship}]->(m)",
                        source_cid = source_cid,
                        target_value = target_value,
                        relationship = key
                    );
                    relationship_queries.push(rel_query);
                }
            };

            for (key, value) in val {
                if key == "cid" || key == "type" {
                    continue;
                }
                if (kind_name == "DataPropertyAssertion"
                    || kind_name == "NegativeDataPropertyAssertion")
                    && key == "target"
                {
                    values.push(format!("n.{0} = '{1}'", key, value.as_str().unwrap()));
                    continue;
                }
                if kind_name == "Annotation" && key == "value" {
                    values.push(format!("n.{0} = '{1}'", key, value.as_str().unwrap()));
                    continue;
                }
                if let Value::Array(array_val) = value {
                    for relationship_value in array_val {
                        if let Value::String(str_val) = relationship_value {
                            add_relationship_value(cid.clone(), key, str_val);
                        }
                    }
                    continue;
                }
                if let Value::String(str_val) = value {
                    add_relationship_value(cid.clone(), key, str_val);
                }
            }
        }

        let mut statement_query = format!(
            "MERGE (n:RlayEntity {{cid: '{1}'}}) SET n.type = '{0}'",
            kind_name, cid
        );
        if !values.is_empty() {
            statement_query.push_str(", ");
            statement_query.push_str(&values.join(", "));
        }

        let mut transaction_queries = vec![statement_query];
        transaction_queries.append(&mut relationship_queries);

        trace!("Insert transaction queries: {:?}", transaction_queries);

        loop {
            let mut client = self.client().await?;

            let mut pipe = redis::pipe();
            // pipe.cmd("MULTI").ignore();
            for query in &transaction_queries {
                pipe.cmd("GRAPH.QUERY")
                    .arg(&self.config.graph_name)
                    .arg(query)
                    .ignore();
            }
            // pipe.cmd("EXEC").ignore();

            match pipe
                .query_async::<_, Option<redis::Value>>(&mut client)
                .await
                .unwrap()
            {
                Option::Some(_) => {
                    break;
                }
                Option::None => {
                    continue;
                }
            }
        }

        Ok(raw_cid)
    }
}

impl BackendFromConfigAndSyncState for RedisgraphBackend {
    type C = RedisgraphBackendConfig;
    type S = SyncState;
    type R = Box<dyn Future<Output = Result<Self, Error>> + Send + Unpin>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R {
        let client_cell = OnceCell::new();
        if let Some(existing_connection) = sync_state.connection_pool {
            let _ = client_cell.set(existing_connection.clone());
        }
        Box::new(future::ok(Self {
            config,
            client: client_cell,
        }))
    }
}

#[derive(Clone)]
pub struct SyncState {
    pub connection_pool: Option<MultiplexedConnection>,
}

#[async_trait]
impl GetEntity for RedisgraphBackend {
    async fn get_entity(&self, _cid: &[u8]) -> Result<Option<Entity>, Error> {
        todo!()
        // future::ready(Ok(self.get(cid).map(|n| n.to_owned()))).boxed()
    }
}

#[async_trait]
impl ResolveEntity for RedisgraphBackend {
    async fn resolve_entity(&self, _cid: &[u8]) -> Result<HashMap<Vec<u8>, Vec<Entity>>, Error> {
        todo!()
    }
}

impl BackendRpcMethodGetEntity for RedisgraphBackend {
    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        Box::pin(Self::get_entity(self, cid.to_owned()))
    }
}

impl BackendRpcMethodStoreEntity for RedisgraphBackend {
    fn store_entity(
        &mut self,
        entity: &Entity,
        _options_object: &Value,
    ) -> BoxFuture<Result<Cid, Error>> {
        Box::pin(self.store_entity(entity.to_owned()))
    }
}

impl BackendRpcMethodNeo4jQuery for RedisgraphBackend {
    fn neo4j_query(&mut self, query: &str) -> BoxFuture<Result<Vec<String>, Error>> {
        Box::pin(self.query_entities(query.to_owned()))
    }
}

impl BackendRpcMethodGetEntities for RedisgraphBackend {}
impl BackendRpcMethodListCids for RedisgraphBackend {}
impl BackendRpcMethodStoreEntities for RedisgraphBackend {}
impl BackendRpcMethodResolveEntity for RedisgraphBackend {}
impl BackendRpcMethodResolveEntities for RedisgraphBackend {}
impl BackendRpcMethods for RedisgraphBackend {}
