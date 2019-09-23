#![recursion_limit = "128"]
#![feature(async_await)]

#[macro_use]
extern crate log;
extern crate failure;
#[macro_use]
extern crate serde_derive;

pub mod config;
mod parse;

use cid::{Cid, ToCid};
use failure::{err_msg, format_err, Error};
use futures::compat::Future01CompatExt;
use futures::future::BoxFuture;
use futures::prelude::*;
use l337::Pool;
use l337_redis::{AsyncConnection, RedisConnectionManager};
use redis::FromRedisValue;
use rlay_backend::BackendRpcMethods;
use rlay_ontology::prelude::*;
use rustc_hex::ToHex;
use serde_json::Value;
use std::sync::Arc;

use crate::config::RedisBackendConfig;
use crate::parse::GetQueryRelationship;

#[derive(Clone)]
pub struct RedisBackend {
    pub config: RedisBackendConfig,
    client: Option<Arc<Pool<RedisConnectionManager>>>,
}

impl RedisBackend {
    pub fn from_config(config: RedisBackendConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    pub async fn client(&mut self) -> Result<l337::Conn<RedisConnectionManager>, Error> {
        if let Some(ref client) = self.client {
            return client
                .connection()
                .compat()
                .map_err(|_| err_msg("Failure getting connection"))
                .await;
        }

        trace!("Creating new connection pool for backend.");
        self.client = Some(Arc::new(self.config.connection_pool().await));
        return self
            .client
            .as_ref()
            .expect("Tried to get non-existent internal connection pool")
            .connection()
            .compat()
            .map_err(|_| err_msg("Failure getting connection"))
            .await;
    }

    async fn get_entity(&mut self, cid: String) -> Result<Option<Entity>, Error> {
        let client = async { self.client().await }.await?;

        let query = format!(
            "MATCH (n:RlayEntity {{ cid: '{0}' }})-[r]->(m) RETURN n,type(r),m",
            cid
        );
        trace!("get_entity query: {:?}", query);

        let query_res: redis::Value = redis::cmd("GRAPH.QUERY")
            .arg(&self.config.graph_name)
            .arg(query)
            .query_async(AsyncConnection(client))
            .compat()
            .await
            .unwrap()
            .1;
        let results_with_meta = Vec::<redis::Value>::from_redis_value(&query_res).unwrap();
        let results = Vec::<redis::Value>::from_redis_value(&results_with_meta[1]).unwrap();

        let relationships: Vec<GetQueryRelationship> = results
            .into_iter()
            .map(|n| GetQueryRelationship::parse(n))
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
            .unwrap();
        let entity = GetQueryRelationship::merge_into_entity(relationships).unwrap();

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

    async fn store_entity(&mut self, entity: Entity) -> Result<Cid, Error> {
        let raw_cid = entity.to_cid().unwrap();
        let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());

        let kind_name: &str = entity.kind().into();
        let entity_val = serde_json::to_value(FormatWeb3(entity.clone())).unwrap();
        let val = entity_val.as_object().unwrap();
        let mut values = Vec::new();
        let mut relationships = Vec::new();
        {
            let mut add_relationship_value = |source_cid, key, target_value| {
                {
                    let rel_query = format!(
                        "MERGE (m:RlayEntity {{ cid: '{target_value}' }})",
                        target_value = target_value
                    );
                    relationships.push(rel_query);
                }
                {
                    let rel_query = format!(
                        "MATCH (n:RlayEntity {{ cid: '{source_cid}'}}),(m:RlayEntity {{ cid: '{target_value}' }}) CREATE (n)-[r:{relationship}]->(m)",
                        source_cid = source_cid,
                        target_value = target_value,
                        relationship = key
                    );
                    relationships.push(rel_query);
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
        trace!("First statement: {}", &statement_query);

        loop {
            let client = async { self.client().await }.await?;

            let mut pipe = redis::pipe();
            pipe.cmd("MULTI").ignore();
            pipe.cmd("GRAPH.QUERY")
                .arg(&self.config.graph_name)
                .arg(statement_query.clone())
                .ignore();
            for relationship in &relationships {
                pipe.cmd("GRAPH.QUERY")
                    .arg(&self.config.graph_name)
                    .arg(relationship)
                    .ignore();
            }
            pipe.cmd("EXEC").ignore();

            match pipe
                .query_async::<_, Option<redis::Value>>(AsyncConnection(client))
                .compat()
                .await
                .unwrap()
                .1
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

impl BackendRpcMethods for RedisBackend {
    fn store_entity(
        &mut self,
        entity: &Entity,
        _options_object: &Value,
    ) -> BoxFuture<Result<Cid, Error>> {
        Box::pin(self.store_entity(entity.to_owned()))
    }

    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        Box::pin(self.get_entity(cid.to_owned()))
    }
}
