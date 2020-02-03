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
use futures::future::BoxFuture;
use futures::prelude::*;
use l337::Pool;
use rlay_backend::rpc::*;
use rlay_backend::{BackendFromConfigAndSyncState, GetEntity};
use rlay_ontology::prelude::*;
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
        let entities = self.get_entities(vec![cid]).await?;
        Ok(entities.get(0).map(|n| n.to_owned()))
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
            "
            UNWIND $cids AS cid
            MATCH (n:RlayEntity {{cid: cid}})-[r]->(m)
            RETURN labels(n),n,type(r),m"
        );
        let statement_query = Statement::new(&query).with_param("cids", &deduped_cids)?;

        trace!("NEO4J QUERY: {:?}", statement_query);
        let start = std::time::Instant::now();
        let query_res = client.exec(statement_query).await?;
        let end = std::time::Instant::now();
        trace!("Query duration: {:?}", end - start);

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
        let cids = self.store_entities(vec![entity]).await?;
        Ok(cids[0].clone())
    }

    async fn store_entities(&mut self, entities: Vec<Entity>) -> Result<Vec<Cid>, Error> {
        let mut entity_objects = Vec::new();
        let client = self.client().await?;

        for entity in entities.iter() {
            let kind_name: &str = entity.kind().into();
            let entity_val = serde_json::to_value(FormatWeb3(entity.clone())).unwrap();
            let mut val = entity_val.as_object().unwrap().to_owned();
            let mut relationships = Vec::new();
            {
                #[derive(Serialize, Deserialize, Debug)]
                struct RelationshipQueryPart {
                    cid: String,
                    kind_name: String,
                }

                // fetch the parts of the payload that should become relationships
                // aka. those that are not self-referencing CIDs
                for (key, value) in val.iter() {
                    if key == "cid" || key == "type" {
                        continue;
                    }
                    if (kind_name == "DataPropertyAssertion"
                        || kind_name == "NegativeDataPropertyAssertion")
                        && key == "target"
                    {
                        continue;
                    }
                    if kind_name == "Annotation" && key == "value" {
                        continue;
                    }
                    if let Value::Array(array_val) = value {
                        for array_val_cid in array_val {
                            relationships.push(RelationshipQueryPart {
                                cid: array_val_cid.as_str().unwrap().to_string(),
                                kind_name: key.clone(),
                            });
                        }
                        continue;
                    }
                    if let Value::String(_) = value {
                        relationships.push(RelationshipQueryPart {
                            cid: value.as_str().unwrap().to_string(),
                            kind_name: key.clone(),
                        });
                    }
                }
            }

            val.insert(
                "relationships".to_string(),
                serde_json::Value::Array(
                    relationships
                        .iter()
                        .map(serde_json::to_value)
                        .map(|r| r.unwrap())
                        .collect(),
                ),
            );
            entity_objects.push(val);
        }

        let sub_query_labels = EntityKind::variants()
            .iter()
            .map(|variant| {
                format!(
                "FOREACH ( ignore in CASE entity.type WHEN '{}' THEN [1] ELSE [] END | SET n:{} )",
                variant, variant)
            })
            .collect::<Vec<String>>()
            .join("\n");

        // collect all possible field names
        let mut all_cid_field_names: Vec<String> = vec![];

        macro_rules! cid_field_names {
            ($kind:path) => {
                all_cid_field_names.extend(
                    <$kind>::cid_field_names()
                        .into_iter()
                        .map(|field| field.to_owned().to_owned())
                        .collect::<Vec<String>>(),
                );
            };
        }

        rlay_ontology::call_with_entity_kinds!(ALL; cid_field_names!);

        all_cid_field_names.sort();
        all_cid_field_names.dedup();

        let sub_query_relations_cids = all_cid_field_names
            .iter()
            .map(|field_name| {
                format!(
                    "FOREACH ( ignore in CASE relationship.kind_name WHEN '{}' THEN [1] ELSE [] END | MERGE (n)-[:{}]->(m) )",
                    field_name, field_name)
            })
            .collect::<Vec<String>>()
            .join("\n");

        // collect all possible data field names
        let mut all_data_field_names: Vec<(String, String)> = vec![];

        macro_rules! data_field_names {
            ($kind:path) => {
                all_data_field_names.extend(
                    <$kind>::data_field_names()
                        .into_iter()
                        .map(|field| {
                            let entity_instance = <$kind>::default();
                            let entity = Into::<Entity>::into(entity_instance);
                            let entity_kind = entity.kind();
                            (
                                Into::<&str>::into(entity_kind).to_owned(),
                                field.to_owned().to_owned(),
                            )
                        })
                        .collect::<Vec<(String, String)>>(),
                );
            };
        }

        rlay_ontology::call_with_entity_kinds!(ALL; data_field_names!);

        let sub_query_relations_data = all_data_field_names
            .iter()
            .map(|field_tuple| {
                format!(
                    "SET (CASE WHEN entity.type = '{}' THEN n END).{} = entity.{}",
                    field_tuple.0, field_tuple.1, field_tuple.1
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        let statement_query_main = format!(
            "
            UNWIND $entities as entity
            MERGE (n:RlayEntity {{cid: entity.cid }})
            {relations_data}
            {labels}
            WITH n, entity
            UNWIND entity.relationships as relationship
                MERGE (m:RlayEntity {{ cid: relationship.cid }})
                {relations_cids}
            RETURN DISTINCT entity.cid",
            labels = sub_query_labels,
            relations_data = sub_query_relations_data,
            relations_cids = sub_query_relations_cids
        );

        let statement_query =
            Statement::new(&statement_query_main).with_param("entities", &entity_objects)?;

        trace!("NEO4J QUERY: {:?}", statement_query);
        let start = std::time::Instant::now();
        client.exec(statement_query).await?;
        let end = std::time::Instant::now();
        trace!("Query duration: {:?}", end - start);

        Ok(entities
            .iter()
            .map(|entity| entity.to_cid().unwrap())
            .collect())
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

impl<'a> GetEntity<'a> for Neo4jBackend {
    type F = BoxFuture<'a, Result<Option<Entity>, Error>>;

    fn get_entity(&'a self, cid: &[u8]) -> Self::F {
        todo!()
        // future::ready(Ok(self.get(cid).map(|n| n.to_owned()))).boxed()
    }
}

impl BackendRpcMethodGetEntity for Neo4jBackend {
    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        Box::pin(self.get_entity(cid.to_owned()))
    }
}

impl BackendRpcMethodStoreEntity for Neo4jBackend {
    fn store_entity(
        &mut self,
        entity: &Entity,
        _options_object: &Value,
    ) -> BoxFuture<Result<Cid, Error>> {
        Box::pin(self.store_entity(entity.to_owned()))
    }
}

impl BackendRpcMethodStoreEntities for Neo4jBackend {
    fn store_entities(
        &mut self,
        entities: &Vec<Entity>,
        _options_object: &Value,
    ) -> BoxFuture<Result<Vec<Cid>, Error>> {
        Box::pin(self.store_entities(entities.to_owned()))
    }
}

impl BackendRpcMethodGetEntities for Neo4jBackend {
    fn get_entities(&mut self, cids: Vec<String>) -> BoxFuture<Result<Vec<Entity>, Error>> {
        Box::pin(self.get_entities(cids))
    }
}

impl BackendRpcMethodListCids for Neo4jBackend {
    fn list_cids(&mut self, entity_kind: Option<&str>) -> BoxFuture<Result<Vec<String>, Error>> {
        let query = match entity_kind {
            None => "MATCH (n:RlayEntity) RETURN DISTINCT n.cid".to_owned(),
            Some(kind) => format!("MATCH (n:RlayEntity:{}) RETURN DISTINCT n.cid", kind),
        };
        self.neo4j_query(&query)
    }
}

impl BackendRpcMethodNeo4jQuery for Neo4jBackend {
    fn neo4j_query(&mut self, query: &str) -> BoxFuture<Result<Vec<String>, Error>> {
        Box::pin(self.query_entities(query.to_owned()))
    }
}

impl BackendRpcMethods for Neo4jBackend {}
