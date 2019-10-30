#![recursion_limit = "128"]

#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;

pub mod config;

use bb8_cypher::CypherConnectionManager;
use cid::{Cid, ToCid};
use failure::{err_msg, Error};
use futures::compat::Future01CompatExt;
use futures::future::BoxFuture;
use futures::prelude::*;
use l337::Pool;
use rlay_backend::{BackendFromConfigAndSyncState, BackendRpcMethods};
use rlay_ontology::prelude::*;
use rustc_hex::ToHex;
use rusted_cypher::cypher::result::Rows;
use rusted_cypher::cypher::Statement;
use rusted_cypher::GraphClient;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use crate::config::Neo4jBackendConfig;

#[derive(Clone)]
pub struct Neo4jBackend {
    pub config: Neo4jBackendConfig,
    client: Option<Arc<Pool<CypherConnectionManager>>>,
}

#[derive(Clone)]
pub struct SyncState {
    pub connection_pool: Option<Arc<Pool<CypherConnectionManager>>>,
}

impl Neo4jBackend {
    pub fn from_config(config: Neo4jBackendConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    pub async fn client(&mut self) -> Result<impl std::ops::Deref<Target = GraphClient>, Error> {
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

    /// Convert rows that has a return statement like `RETURN labels(n),n,type(r),m` into entities
    fn rows_to_entity(rows: Rows) -> Vec<Entity> {
        let mut entity_map = HashMap::<String, Value>::new();

        for row in rows {
            let labels: Value = row.get("labels(n)").unwrap();
            let label = labels
                .as_array()
                .unwrap()
                .into_iter()
                // TODO: make more robust against additional labels
                // currently only filter out the known extra label RlayEntity that is used for
                // a index
                .filter(|n| n.as_str().unwrap() != "RlayEntity")
                .collect::<Vec<_>>()[0]
                .clone();
            // build empty entity with which we can check if fields are supposed to be arrays
            let entity_kind = EntityKind::from_name(label.as_str().unwrap()).unwrap();
            let empty_entity: Value =
                serde_json::to_value(FormatWeb3(entity_kind.empty_entity())).unwrap();

            let main_entity_cid: String = row
                .get::<Value>("n")
                .expect("Unable to get value n in neo4j result set")
                .as_object_mut()
                .expect("Unable to convert return neo4j return value to object")
                .get("cid")
                .expect("Neo4j return value does not contain \"cid\"")
                .as_str()
                .expect("Unable to convert neo4j return value n.cid to string")
                .to_owned();
            let entity = entity_map.entry(main_entity_cid).or_insert_with(|| {
                let mut main_entity: Value = row.get("n").unwrap();
                main_entity["type"] = label;
                main_entity.as_object_mut().unwrap().remove("cid");

                main_entity
            });

            let value_value: Value = row.get("m").unwrap();
            let value_cid = value_value["cid"].clone();

            let rel_type_value: Value = row.get("type(r)").unwrap();
            let rel_type = rel_type_value.as_str().unwrap().clone();

            match empty_entity[rel_type] {
                Value::Array(_) => {
                    if !entity[rel_type].is_array() {
                        entity[rel_type] = Value::Array(Vec::new());
                    }
                    entity[rel_type].as_array_mut().unwrap().push(value_cid);
                }
                Value::String(_) => {
                    entity[rel_type] = value_cid;
                }
                Value::Null => {
                    entity[rel_type] = value_cid;
                }
                _ => unimplemented!(),
            }
        }

        entity_map
            .values()
            .into_iter()
            .map(|entity| {
                let web3_entity: FormatWeb3<Entity> =
                    serde_json::from_value((*entity).clone()).unwrap();
                let entity: Entity = web3_entity.0;

                entity
            })
            .collect()
    }

    async fn get_entity(&mut self, cid: String) -> Result<Option<Entity>, Error> {
        let client = self.client().err_into::<Error>().await?;

        let query = format!(
            "MATCH (n:RlayEntity {{ cid: \"{0}\" }})-[r]->(m) RETURN labels(n),n,type(r),m",
            cid
        );
        trace!("get_entity query: {:?}", query);

        let query_res = client.exec(query).await?;
        if query_res.rows().count() == 0 {
            return Ok(None);
        }

        let entity = Self::rows_to_entity(query_res.rows())
            .get(0)
            .unwrap()
            .to_owned();

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
        let client = self.client().await?;

        let deduped_cids = {
            let mut deduped_cids = cids.to_owned();
            deduped_cids.dedup();
            deduped_cids
        };

        let query = format!(
            "MATCH (n:RlayEntity)-[r]->(m) WHERE n.cid IN {0:?} RETURN labels(n),n,type(r),m",
            deduped_cids,
        );
        trace!("get_entities query: \"{}\"", query);

        let query_res = client.exec(query).await?;

        if query_res.rows().count() == 0 {
            return Ok(vec![]);
        }

        let entities = Self::rows_to_entity(query_res.rows());
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
        let client = self.client().await?;

        let query_res = client.exec(query).await?;
        let cids: Vec<_> = query_res.rows().map(|row| row.get_n(0).unwrap()).collect();

        Ok(cids)
    }

    async fn store_entity(&mut self, entity: Entity) -> Result<Cid, Error> {
        let raw_cid = entity.to_cid().unwrap();
        let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());

        let client = self.client().await?;

        let kind_name: &str = entity.kind().into();
        let entity_val = serde_json::to_value(FormatWeb3(entity.clone())).unwrap();
        let val = entity_val.as_object().unwrap();
        let mut values = Vec::new();
        let mut relationships = Vec::new();
        {
            let mut add_relationship_value = |cid, key, value| {
                let rel_query_main = format!(
                    "MATCH (n:RlayEntity {{ cid: {{cid1}} }}) MERGE (m:RlayEntity {{ cid: {{cid2}} }}) MERGE (n)-[r:{0}]->(m)",
                    key
                );
                let rel_query = Statement::new(rel_query_main)
                    .with_param("cid1", &cid).unwrap()
                    .with_param("cid2", value).unwrap();
                relationships.push(rel_query);
            };

            struct StatementQueryPart {
                query_sub: String,
                param_key: String,
                param_val: String,
            }

            for (key, value) in val {
                if key == "cid" || key == "type" {
                    continue;
                }
                if (kind_name == "DataPropertyAssertion"
                    || kind_name == "NegativeDataPropertyAssertion")
                    && key == "target"
                {
                    values.push(StatementQueryPart {
                        query_sub: "n.target = {datapropVal}".to_owned(),
                        param_key: "datapropVal".to_owned(),
                        param_val: value.as_str().unwrap().to_owned()
                    });
                    continue;
                }
                if kind_name == "Annotation" && key == "value" {
                    values.push(StatementQueryPart {
                        query_sub: "n.value = {annotationVal}".to_owned(),
                        param_key: "annotationVal".to_owned(),
                        param_val: value.as_str().unwrap().to_owned()
                    });
                    continue;
                }
                if let Value::Array(array_val) = value {
                    for relationship_value in array_val {
                        add_relationship_value(cid.clone(), key, relationship_value);
                    }
                    continue;
                }
                if let Value::String(_) = value {
                    add_relationship_value(cid.clone(), key, value);
                }
            }
        }

        let mut statement_query_main = format!(
            "MERGE (n:RlayEntity {{cid: {{cid}} }}) SET n:{0}",
            kind_name
        );
        let mut statement_query = Statement::new(&statement_query_main)
            .with_param("cid", &cid)?;

        if !values.is_empty() {
            for value in values.iter() {
                statement_query_main.push_str(", ");
                statement_query_main.push_str(&value.query_sub);
            }
            statement_query = Statement::new(&statement_query_main)
                .with_param("cid", &cid)?;
            for value in values.iter() {
                statement_query = statement_query.clone()
                    .with_param(&value.param_key, &value.param_val).unwrap()
            }
        }

        let (mut transaction, _) = client.transaction().begin().await?;

        // let mut query = client.query();
        trace!("NEO4J QUERY: {:?}", statement_query);
        transaction.add_statement(statement_query);
        for relationship in relationships {
            trace!("NEO4J QUERY: {:?}", relationship);
            transaction.add_statement(relationship);
        }

        let start = std::time::Instant::now();
        transaction.commit().await?;
        let end = std::time::Instant::now();
        trace!("Query duration: {:?}", end - start);

        Ok(raw_cid)
    }
}

impl BackendFromConfigAndSyncState for Neo4jBackend {
    type C = Neo4jBackendConfig;
    type S = SyncState;
    type R = Box<dyn Future<Output = Result<Self, Error>> + Send + Unpin>;

    fn from_config_and_syncstate(config: Self::C, sync_state: Self::S) -> Self::R {
        Box::new(future::ok(Self {
            config,
            client: sync_state.connection_pool.clone(),
        }))
    }
}

impl BackendRpcMethods for Neo4jBackend {
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

    fn list_cids(&mut self, entity_kind: Option<&str>) -> BoxFuture<Result<Vec<String>, Error>> {
        let query = match entity_kind {
            None => "MATCH (n:RlayEntity) RETURN DISTINCT n.cid".to_owned(),
            Some(kind) => format!("MATCH (n:RlayEntity:{}) RETURN DISTINCT n.cid", kind),
        };
        self.neo4j_query(&query)
    }

    fn neo4j_query(&mut self, query: &str) -> BoxFuture<Result<Vec<String>, Error>> {
        Box::pin(self.query_entities(query.to_owned()))
    }
}
