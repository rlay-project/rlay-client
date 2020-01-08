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
use rlay_backend::BackendFromConfigAndSyncState;
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

    fn pattern_object_to_cid(object: &Value) -> String {
        let self_value: Value = object["self"].clone();

        self_value
            .clone()
            .as_object_mut()
            .expect("Unable to convert return neo4j return value to object")
            .get("cid")
            .expect("Neo4j return value does not contain \"cid\"")
            .as_str()
            .expect("Unable to convert neo4j return value n.cid to string")
            .to_owned()
    }

    fn pattern_object_to_label(object: &Value) -> Option<String> {
        let self_labels: Value = object["self_labels"].clone();

        let parsed_labels = self_labels
            .as_array()
            .unwrap()
            .into_iter()
            // TODO: make more robust against additional labels
            // currently only filter out the known extra label RlayEntity that is used for
            // a index
            .filter(|n| n.as_str().unwrap() != "RlayEntity")
            .collect::<Vec<_>>();

        match parsed_labels.len() > 0 {
            true => Some(parsed_labels[0].clone().as_str().unwrap().to_owned()),
            false => None,
        }
    }

    fn pattern_object_to_relationship_cid_tuple(object: &Value) -> (String, String) {
        let rel_type_value: Value = object["rel_type"].clone();
        let rel_type = rel_type_value.as_str().unwrap().to_owned();
        let cid = Self::pattern_object_to_cid(object);
        (rel_type, cid)
    }

    fn pattern_objects_to_relationship_cid_tuples(objects: &Value) -> Vec<(String, String)> {
        let mut tuples: Vec<(String, String)> = vec![];
        if objects.is_array() {
            tuples = objects
                .as_array()
                .unwrap()
                .iter()
                .map(|pattern_object| {
                    Self::pattern_object_to_relationship_cid_tuple(pattern_object)
                })
                .collect();
        }
        tuples
    }

    /// Convert a "pattern comprehension" object (part of a row - see query for details) to entity
    fn pattern_object_to_entity(object: &Value) -> Option<Entity> {
        let self_value: Value = object["self"].clone();
        let properties: Value = object["properties"].clone();

        let self_label_option = Self::pattern_object_to_label(object);
        let self_label = match self_label_option {
            // it is a leaf node that can not be turned into an entity
            None => return None,
            Some(val) => val,
        };
        let rel_cid_tuples = Self::pattern_objects_to_relationship_cid_tuples(&properties);

        let mut entity = self_value;
        entity["type"] = self_label.clone().into();
        entity.as_object_mut().unwrap().remove("cid");

        // build empty entity with which we can check if fields are supposed to be arrays
        let entity_kind = EntityKind::from_name(self_label.as_str()).unwrap();
        let empty_entity: Value =
            serde_json::to_value(FormatWeb3(entity_kind.empty_entity())).unwrap();

        for rel_cid_tuple in &rel_cid_tuples {
            match empty_entity[rel_cid_tuple.0.as_str()] {
                Value::Array(_) => {
                    if !entity[rel_cid_tuple.0.as_str()].is_array() {
                        entity[rel_cid_tuple.0.as_str()] = Value::Array(Vec::new());
                    }
                    entity[rel_cid_tuple.0.as_str()]
                        .as_array_mut()
                        .unwrap()
                        .push(rel_cid_tuple.1.as_str().into());
                }
                Value::String(_) => {
                    entity[rel_cid_tuple.0.as_str()] = rel_cid_tuple.1.as_str().into();
                }
                Value::Null => {
                    entity[rel_cid_tuple.0.as_str()] = rel_cid_tuple.1.as_str().into();
                }
                _ => unimplemented!(),
            }
        }

        let web3_entity: FormatWeb3<Entity> = serde_json::from_value(entity).unwrap();
        Some(web3_entity.0)
    }

    /// Convert a "pattern comprehension" object (part of a row - see query for details) to entity
    fn pattern_object_to_entities(object: &Value) -> Vec<Entity> {
        let entity = Self::pattern_object_to_entity(object);

        let properties: Value = object["properties"].clone();
        let mut property_entities = Self::pattern_objects_to_entities(&properties);

        let assertions: Value = object["assertions"].clone();
        let mut assertion_entities = Self::pattern_objects_to_entities(&assertions);

        let mut entity_vec: Vec<Entity> = vec![];
        match entity {
            None => (),
            Some(entity) => entity_vec.push(entity),
        }
        entity_vec.append(&mut property_entities);
        entity_vec.append(&mut assertion_entities);
        entity_vec
    }

    /// Convert a list of "pattern comprehension" objects to entities
    /// see usage in pattern_object_to_entities (recursive usage)
    fn pattern_objects_to_entities(objects: &Value) -> Vec<Entity> {
        let mut entities: Vec<Entity> = vec![];
        if objects.is_array() {
            entities = objects
                .as_array()
                .unwrap()
                .iter()
                .map(|pattern_object| Self::pattern_object_to_entities(pattern_object))
                .fold(vec![], |mut all_entities, entities| {
                    all_entities.extend(entities);
                    all_entities
                });
        }
        entities
    }

    /// Convert "pattern comprehension" rows to entities
    fn pattern_rows_to_entities(rows: Rows) -> Vec<Entity> {
        rows.map(|row| {
            let data: Value = row.get("data").unwrap();
            Self::pattern_object_to_entities(&data)
        })
        .fold(vec![], |mut all_entities, entities| {
            all_entities.extend(entities);
            all_entities
        })
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

    async fn resolve_entity(&mut self, cid: String) -> Result<Option<Entity>, Error> {
        let entities = self.resolve_entities(vec![cid]).await?;
        Ok(entities.get(0).map(|n| n.clone()))
    }

    pub async fn resolve_entities(&mut self, cids: Vec<String>) -> Result<Vec<Entity>, Error> {
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
            MATCH (n0:RlayEntity {{cid: cid}})
            RETURN n0 {{
                self: n0,
                self_labels: labels(n0),
                properties: [(n0)-[r0]->(n1) | {{
                    rel_type: type(r0),
                    self: n1,
                    self_labels: labels(n1),
                    properties: [(n1)-[r1]->(n2) | {{
                        rel_type: type(r1),
                        self: n2,
                        self_labels: labels(n2)
                    }}],
                    assertions: CASE single(x IN labels(n0) WHERE x = 'Individuals') WHEN TRUE
                        THEN [(n1)<-[r1]-(n2) | {{
                            rel_type: type(r1),
                            self: n2,
                            self_labels: labels(n2)
                        }}]
                        END
                    }}],
                assertions: CASE single(x IN labels(n0) WHERE x = 'Individuals') WHEN TRUE
                    THEN [(n0)<-[r0]-(n1) | {{
                        rel_type: type(r0),
                        self: n1,
                        self_labels: labels(n1)
                    }}]
                    END
            }} as data"
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

        let entities = Self::pattern_rows_to_entities(query_res.rows());
        trace!("resolve_entities retrieved {} entities", entities.len());
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

impl BackendRpcMethods for Neo4jBackend {
    fn resolve_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        Box::pin(self.resolve_entity(cid.to_owned()))
    }

    fn resolve_entities(&mut self, cids: Vec<String>) -> BoxFuture<Result<Vec<Entity>, Error>> {
        Box::pin(self.resolve_entities(cids))
    }
}
