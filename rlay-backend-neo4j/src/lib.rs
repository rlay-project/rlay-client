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

        let query = format!("
            UNWIND $cids AS cid
            MATCH (n:RlayEntity {{cid: cid}})-[r]->(m)
            RETURN labels(n),n,type(r),m"
        );
        let statement_query = Statement::new(&query)
            .with_param("cids", &deduped_cids)?;

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
                    kind_name: String
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
                            relationships.push(RelationshipQueryPart{
                                cid: array_val_cid.as_str().unwrap().to_string(),
                                kind_name: key.clone()
                            });
                        }
                        continue;
                    }
                    if let Value::String(_) = value {
                        relationships.push(RelationshipQueryPart{
                            cid: value.as_str().unwrap().to_string(),
                            kind_name: key.clone()
                        });
                    }
                }
            }

            val.insert("relationships".to_string(), serde_json::Value::Array(
                relationships
                    .iter()
                    .map(serde_json::to_value)
                    .map(|r| r.unwrap())
                    .collect()
            ));
            entity_objects.push(val);
        }

        let statement_query_main = format!("
            UNWIND $entities as entity
            MERGE (n:RlayEntity {{cid: entity.cid }})
            SET (CASE WHEN entity.type = 'Annotation' THEN n END).value = entity.value
            SET (CASE WHEN entity.type = 'DataPropertyAssertion' THEN n END).target = entity.target
            SET (CASE WHEN entity.type = 'NegativeDataPropertyAssertion' THEN n END).target = entity.target
            FOREACH ( ignore in CASE entity.type WHEN 'Class' THEN [1] ELSE [] END | SET n:Class )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectIntersectionOf' THEN [1] ELSE [] END | SET n:ObjectIntersectionOf )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectUnionOf' THEN [1] ELSE [] END | SET n:ObjectUnionOf )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectComplementOf' THEN [1] ELSE [] END | SET n:ObjectComplementOf )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectOneOf' THEN [1] ELSE [] END | SET n:ObjectOneOf )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectSomeValuesFrom' THEN [1] ELSE [] END | SET n:ObjectSomeValuesFrom )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectAllValuesFrom' THEN [1] ELSE [] END | SET n:ObjectAllValuesFrom )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectHasValue' THEN [1] ELSE [] END | SET n:ObjectHasValue )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectHasSelf' THEN [1] ELSE [] END | SET n:ObjectHasSelf )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectMinCardinality' THEN [1] ELSE [] END | SET n:ObjectMinCardinality )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectMaxCardinality' THEN [1] ELSE [] END | SET n:ObjectMaxCardinality )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectExactCardinality' THEN [0] ELSE [] END | SET n:ObjectExactCardinality )
            FOREACH ( ignore in CASE entity.type WHEN 'DataSomeValuesFrom' THEN [1] ELSE [] END | SET n:DataSomeValuesFrom )
            FOREACH ( ignore in CASE entity.type WHEN 'DataAllValuesFrom' THEN [1] ELSE [] END | SET n:DataAllValuesFrom )
            FOREACH ( ignore in CASE entity.type WHEN 'DataHasValue' THEN [1] ELSE [] END | SET n:DataHasValue )
            FOREACH ( ignore in CASE entity.type WHEN 'DataMinCardinality' THEN [1] ELSE [] END | SET n:DataMinCardinality )
            FOREACH ( ignore in CASE entity.type WHEN 'DataMaxCardinality' THEN [1] ELSE [] END | SET n:DataMaxCardinality )
            FOREACH ( ignore in CASE entity.type WHEN 'DataExactCardinality' THEN [1] ELSE [] END | SET n:DataExactCardinality )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectProperty' THEN [1] ELSE [] END | SET n:ObjectProperty )
            FOREACH ( ignore in CASE entity.type WHEN 'InverseObjectProperty' THEN [0] ELSE [] END | SET n:InverseObjectProperty )
            FOREACH ( ignore in CASE entity.type WHEN 'DataProperty' THEN [1] ELSE [] END | SET n:DataProperty )
            FOREACH ( ignore in CASE entity.type WHEN 'Annotation' THEN [1] ELSE [] END | SET n:Annotation )
            FOREACH ( ignore in CASE entity.type WHEN 'Individual' THEN [1] ELSE [] END | SET n:Individual )
            FOREACH ( ignore in CASE entity.type WHEN 'AnnotationProperty' THEN [1] ELSE [] END | SET n:AnnotationProperty )
            FOREACH ( ignore in CASE entity.type WHEN 'ClassAssertion' THEN [1] ELSE [] END | SET n:ClassAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'NegativeClassAssertion' THEN [1] ELSE [] END | SET n:NegativeClassAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'ObjectPropertyAssertion' THEN [1] ELSE [] END | SET n:ObjectPropertyAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'NegativeObjectPropertyAssertion' THEN [1] ELSE [] END | SET n:NegativeObjectPropertyAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'DataPropertyAssertion' THEN [1] ELSE [] END | SET n:DataPropertyAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'NegativeDataPropertyAssertion' THEN [1] ELSE [] END | SET n:NegativeDataPropertyAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'AnnotationAssertion' THEN [1] ELSE [] END | SET n:AnnotationAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'NegativeAnnotationAssertion' THEN [1] ELSE [] END | SET n:NegativeAnnotationAssertion )
            FOREACH ( ignore in CASE entity.type WHEN 'Literal' THEN [1] ELSE [] END | SET n:Literal )
            FOREACH ( ignore in CASE entity.type WHEN 'Datatype' THEN [1] ELSE [] END | SET n:Datatype )
            FOREACH ( ignore in CASE entity.type WHEN 'DataIntersectionOf' THEN [1] ELSE [] END | SET n:DataIntersectionOf )
            FOREACH ( ignore in CASE entity.type WHEN 'DataUnionOf' THEN [1] ELSE [] END | SET n:DataUnionOf )
            FOREACH ( ignore in CASE entity.type WHEN 'DataComplementOf' THEN [1] ELSE [] END | SET n:DataComplementOf )
            FOREACH ( ignore in CASE entity.type WHEN 'DataOneOf' THEN [1] ELSE [] END | SET n:DataOneOf )
            WITH n, entity
            UNWIND entity.relationships as relationship
                MERGE (m:RlayEntity {{ cid: relationship.cid }})
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'annotations' THEN [1] ELSE [] END | MERGE (n)-[:annotations]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'superClassExpression' THEN [0] ELSE [] END | MERGE (n)-[:superClassExpression]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'complementOf' THEN [1] ELSE [] END | MERGE (n)-[:complementOf]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'superObjectPropertyExpression' THEN [0] ELSE [] END | MERGE (n)-[:superObjectPropertyExpression]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'superDataPropertyExpression' THEN [1] ELSE [] END | MERGE (n)-[:superDataPropertyExpression]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'domain' THEN [1] ELSE [] END | MERGE (n)-[:domain]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'range' THEN [1] ELSE [] END | MERGE (n)-[:range]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'property' THEN [0] ELSE [] END | MERGE (n)-[:property]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'value' THEN [1] ELSE [] END | MERGE (n)-[:value]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'class_assertions' THEN [0] ELSE [] END | MERGE (n)-[:class_assertions]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'negative_class_assertions' THEN [1] ELSE [] END | MERGE (n)-[:negative_class_assertions]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'object_property_assertions' THEN [1] ELSE [] END | MERGE (n)-[:object_property_assertions]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'negative_object_property_assertions' THEN [1] ELSE [] END | MERGE (n)-[:negative_object_property_assertions]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'data_property_assertions' THEN [0] ELSE [] END | MERGE (n)-[:data_property_assertions]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'negative_data_property_assertions' THEN [1] ELSE [] END | MERGE (n)-[:negative_data_property_assertions]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'subject' THEN [1] ELSE [] END | MERGE (n)-[:subject]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'class' THEN [1] ELSE [] END | MERGE (n)-[:class]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'target' THEN [1] ELSE [] END | MERGE (n)-[:target]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'datatype' THEN [1] ELSE [] END | MERGE (n)-[:datatype]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'datatypes' THEN [1] ELSE [] END | MERGE (n)-[:datatypes]->(m) )
                FOREACH ( ignore in CASE relationship.kind_name WHEN 'values' THEN [1] ELSE [] END | MERGE (n)-[:values]->(m) )
            RETURN DISTINCT entity.cid",
        );

        let statement_query = Statement::new(&statement_query_main)
            .with_param("entities", &entity_objects)?;

        trace!("NEO4J QUERY: {:?}", statement_query);
        let start = std::time::Instant::now();
        client.exec(statement_query).await?;
        let end = std::time::Instant::now();
        trace!("Query duration: {:?}", end - start);


        Ok(entities.iter().map(|entity| entity.to_cid().unwrap()).collect())
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

    fn store_entities(
        &mut self,
        entities: &Vec<Entity>,
        _options_object: &Value,
    ) -> BoxFuture<Result<Vec<Cid>, Error>> {
        Box::pin(self.store_entities(entities.to_owned()))
    }

    fn get_entity(&mut self, cid: &str) -> BoxFuture<Result<Option<Entity>, Error>> {
        Box::pin(self.get_entity(cid.to_owned()))
    }

    fn get_entities(&mut self, cids: Vec<String>) -> BoxFuture<Result<Vec<Entity>, Error>> {
        Box::pin(self.get_entities(cids))
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
