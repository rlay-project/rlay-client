#![recursion_limit = "128"]

#[macro_use]
extern crate log;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
extern crate static_assertions as sa;

pub mod config;

use bb8_cypher::CypherConnectionManager;
use cid::{Cid, ToCid};
use failure::Error;
use futures::future::BoxFuture;
use futures::prelude::*;
use l337::Pool;
use once_cell::sync::OnceCell;
use rlay_backend::rpc::*;
use rlay_backend::BackendFromConfigAndSyncState;
use rlay_ontology::prelude::*;
use rusted_cypher::cypher::result::Rows;
use rusted_cypher::cypher::Statement;
use rusted_cypher::GraphClient;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::future::Future;

use crate::config::Neo4jBackendConfig;

sa::assert_impl_all!(Neo4jBackend: Send, Sync);
#[derive(Clone)]
pub struct Neo4jBackend {
    pub config: Neo4jBackendConfig,
    client: OnceCell<Pool<CypherConnectionManager>>,
}

#[derive(Clone)]
pub struct SyncState {
    pub connection_pool: Option<Pool<CypherConnectionManager>>,
}

/// Map with CID of resolved entity as key, and Vec of all contained entities within the resolved
/// entity as values.
type ResolvedEntities = HashMap<String, Vec<Entity>>;

impl Neo4jBackend {
    pub fn from_config(config: Neo4jBackendConfig) -> Self {
        Self {
            config,
            client: OnceCell::new(),
        }
    }

    pub async fn client(&self) -> Result<impl std::ops::Deref<Target = GraphClient>, Error> {
        if let Some(client) = self.client.get() {
            return Ok(client.connection().await.unwrap());
        }

        trace!("Creating new connection pool for backend.");
        let new_connection = self.config.connection_pool().await;
        let _ = self.client.set(new_connection.clone());
        Ok(new_connection.connection().await.unwrap())
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
        let children: Value = object["children"].clone();

        let self_label_option = Self::pattern_object_to_label(object);
        let self_label = match self_label_option {
            // it is a leaf node that can not be turned into an entity
            None => return None,
            Some(val) => val,
        };
        let rel_cid_tuples = Self::pattern_objects_to_relationship_cid_tuples(&children);
        match rel_cid_tuples[..] {
            // it is not fully resolved so we do not return an entity
            [] => return None,
            _ => (),
        }

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

        let web3_entity: FormatWeb3<Entity> = match serde_json::from_value(entity) {
            Err(_) => return None,
            Ok(v) => v,
        };
        Some(web3_entity.0)
    }

    /// Convert a "pattern comprehension" object (part of a row - see query for details) to entity
    fn pattern_object_to_entities(object: &Value) -> Vec<Entity> {
        let entity = Self::pattern_object_to_entity(object);

        let children: Value = object["children"].clone();
        let mut children_entities = Self::pattern_objects_to_entities(&children);

        let mut entity_vec: Vec<Entity> = vec![];
        match entity {
            None => (),
            Some(entity) => entity_vec.push(entity),
        }
        entity_vec.append(&mut children_entities);
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

    /// Convert "pattern comprehension" rows to resolved entities hash map
    fn pattern_rows_to_resolve_entities(rows: Rows) -> ResolvedEntities {
        let mut resolve_entities_map = HashMap::<String, Vec<Entity>>::new();
        for row in rows {
            let cid: String = row.get("cid").unwrap();
            let data: Value = row.get("data").unwrap();
            resolve_entities_map.insert(cid, Self::pattern_object_to_entities(&data));
        }
        resolve_entities_map
    }

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

    async fn get_entity(&self, cid: String) -> Result<Option<Entity>, Error> {
        let entities = Self::get_entities(self, vec![cid]).await?;
        Ok(entities.get(0).map(|n| n.to_owned()))
    }

    pub async fn get_entities(&self, cids: Vec<String>) -> Result<Vec<Entity>, Error> {
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
            RETURN
            cid as cid,
            {{
                self: n0,
                self_labels: labels(n0),
                children: [(n0:RlayEntity)-[r0]->(n1:RlayEntity) | {{
                    rel_type: type(r0),
                    self: n1,
                    self_labels: labels(n1)
                }}]
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
        trace!("get_entities retrieved {} entities", entities.len());
        debug_assert!(
            deduped_cids.len() == entities.len(),
            "{} cids provided and {} entities retrieved",
            deduped_cids.len(),
            entities.len()
        );

        Ok(entities)
    }

    async fn resolve_entity(&mut self, cid: String) -> Result<ResolvedEntities, Error> {
        self.resolve_entities(vec![cid]).await
    }

    pub async fn resolve_entities(&mut self, cids: Vec<String>) -> Result<ResolvedEntities, Error> {
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
            RETURN
            cid as cid,
            {{
                self: n0,
                self_labels: labels(n0),
                children: [(n0:RlayEntity)-[r0]->(n1:RlayEntity) | {{
                    rel_type: type(r0),
                    self: n1,
                    self_labels: labels(n1),
                    children: [(n1:RlayEntity)-[r1]->(n2:RlayEntity) | {{
                        rel_type: type(r1),
                        self: n2,
                        self_labels: labels(n2),
                        children: CASE single(x IN labels(n2) WHERE x = 'Individual' AND n2 <> n0) WHEN TRUE
                            THEN [(n2:RlayEntity)-[r2]->(n3:RlayEntity) | {{
                                rel_type: type(r2),
                                self: n3,
                                self_labels: labels(n3)
                            }}]
                            END
                    }}]
                }}] +
                [(n0:RlayEntity)<-[r0:subject]-(n1:RlayEntity) | {{
                    rel_type: type(r0),
                    self: n1,
                    self_labels: labels(n1),
                    children: [(n1:RlayEntity)-[r1]->(n2:RlayEntity) | {{
                        rel_type: type(r1),
                        self: n2,
                        self_labels: labels(n2),
                        children: CASE single(x IN labels(n2) WHERE x = 'Individual' AND n2 <> n0) WHEN TRUE
                            THEN [(n2:RlayEntity)-[r2]->(n3:RlayEntity) | {{
                                rel_type: type(r2),
                                self: n3,
                                self_labels: labels(n3)
                            }}]
                            END
                    }}]
                }}]
            }} as data"
        );
        let statement_query = Statement::new(&query).with_param("cids", &deduped_cids)?;

        trace!("NEO4J QUERY: {:?}", statement_query);
        let start = std::time::Instant::now();
        let query_res = client.exec(statement_query).await?;
        let end = std::time::Instant::now();
        trace!("Query duration: {:?}", end - start);

        let entities = Self::pattern_rows_to_resolve_entities(query_res.rows());
        trace!("resolve_entities retrieved {} entities", entities.len());

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

impl BackendRpcMethodGetEntity for Neo4jBackend {
    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        Box::pin(Self::get_entity(self, cid.to_owned()))
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
        Box::pin(Self::get_entities(self, cids))
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

impl BackendRpcMethodResolveEntity for Neo4jBackend {
    fn resolve_entity(
        &mut self,
        cid: &str,
    ) -> BoxFuture<Result<HashMap<String, Vec<Entity>>, Error>> {
        Box::pin(self.resolve_entity(cid.to_owned()))
    }
}

impl BackendRpcMethodResolveEntities for Neo4jBackend {
    fn resolve_entities(
        &mut self,
        cids: Vec<String>,
    ) -> BoxFuture<Result<HashMap<String, Vec<Entity>>, Error>> {
        Box::pin(self.resolve_entities(cids))
    }
}

impl BackendRpcMethods for Neo4jBackend {}
