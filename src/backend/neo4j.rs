use cid::{Cid, ToCid};
#[allow(unused_imports)]
use failure::{err_msg, Error};
use rlay_ontology::prelude::*;
use rustc_hex::ToHex;
use rusted_cypher::cypher::result::Rows;
use rusted_cypher::cypher::Statement;
use rusted_cypher::GraphClient;
use serde_json::{self, Value};
use std::collections::HashMap;

use crate::backend::{BackendFromConfig, BackendFromConfigAndSyncState, BackendRpcMethods};
use crate::config::backend::Neo4jBackendConfig;

pub struct Neo4jBackend {
    pub config: Neo4jBackendConfig,
    client: Option<GraphClient>,
}

impl Neo4jBackend {
    pub fn client(&mut self) -> &GraphClient {
        if let Some(ref client) = self.client {
            return client;
        }

        self.client = Some(self.config.client().unwrap());
        return self.client.as_ref().unwrap();
    }

    /// Convert rows that has a return statement like `RETURN labels(n),n,type(r),m` into entities
    fn rows_to_entity(rows: Rows) -> Vec<Entity> {
        let mut entity_map = HashMap::<String, Value>::new();

        for row in rows {
            let labels: Value = row.get("labels(n)").unwrap();
            let label = labels.as_array().unwrap()[0].clone();
            // build empty entity with which we can check if fields are supposed to be arrays
            let entity_kind = EntityKind::from_name(label.as_str().unwrap()).unwrap();
            let empty_entity: Value =
                serde_json::to_value(FormatWeb3(entity_kind.empty_entity())).unwrap();

            let main_entity_cid: String = row
                .get::<Value>("n")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .get("cid")
                .unwrap()
                .as_str()
                .unwrap()
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

    pub fn get_entities(&mut self, cids: &[String]) -> Result<Vec<Entity>, Error> {
        let client = self.client();
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
        let query_res = client.exec(query).unwrap();
        if query_res.rows().count() == 0 {
            return Ok(vec![]);
        }

        let entities = Self::rows_to_entity(query_res.rows());
        debug_assert!(
            deduped_cids.len() == entities.len(),
            "{} cids provided and {} entities retrieved",
            deduped_cids.len(),
            entities.len()
        );

        Ok(entities)
    }
}

impl BackendFromConfig for Neo4jBackend {
    type C = Neo4jBackendConfig;

    fn from_config(config: Self::C) -> Result<Self, Error> {
        Ok(Self {
            config,
            client: None,
        })
    }
}

impl BackendFromConfigAndSyncState for Neo4jBackend {
    type C = Neo4jBackendConfig;
    type S = ();

    fn from_config_and_syncstate(config: Self::C, _sync_state: Self::S) -> Result<Self, Error> {
        Ok(Self {
            config,
            client: None,
        })
    }
}

impl BackendRpcMethods for Neo4jBackend {
    fn store_entity(&mut self, entity: &Entity, _options_object: &Value) -> Result<Cid, Error> {
        let raw_cid = entity.to_cid().unwrap();
        let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());

        let client = self.client();
        let kind_name: &str = entity.kind().into();
        let entity_val = serde_json::to_value(FormatWeb3(entity.clone())).unwrap();
        let val = entity_val.as_object().unwrap();
        let mut values = Vec::new();
        let mut relationships = Vec::new();
        {
            let mut add_relationship_value = |cid, key, value| {
                let rel_query = format!(
                    "MATCH (n:RlayEntity {{ cid: \"{0}\"}}) MERGE (m {{ cid: {2} }}) MERGE (n)-[r:{1}]->(m)",
                    cid, key, value
                );
                relationships.push(rel_query);
            };

            for (key, value) in val {
                if key == "cid" || key == "type" {
                    continue;
                }
                if (kind_name == "DataPropertyAssertion"
                    || kind_name == "NegativeDataPropertyAssertion")
                    && key == "target"
                {
                    values.push(format!("n.{0} = {1}", key, value));
                    continue;
                }
                if kind_name == "Annotation" && key == "value" {
                    values.push(format!("n.{0} = {1}", key, value));
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

        let mut statement_query = format!(
            "MERGE (n:RlayEntity {{cid: \"{1}\"}}) SET n:{0}",
            kind_name, cid
        );
        if !values.is_empty() {
            statement_query.push_str(", ");
            statement_query.push_str(&values.join(", "));
        }

        trace!("NEO4J QUERY: {}", statement_query);
        let mut query = client.query();
        query.add_statement(Statement::new(statement_query));
        for relationship in relationships {
            trace!("NEO4J QUERY: {}", relationship);
            query.add_statement(Statement::new(relationship));
        }
        query.send().unwrap();

        Ok(raw_cid)
    }

    fn get_entity(&mut self, cid: &str) -> Result<Option<Entity>, Error> {
        let client = self.client();

        let query = format!(
            "MATCH (n:RlayEntity {{ cid: \"{0}\" }})-[r]->(m) RETURN labels(n),n,type(r),m",
            cid
        );
        trace!("get_entity query: {:?}", query);
        let query_res = client.exec(query).unwrap();
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

    fn neo4j_query(&mut self, query: &str) -> Result<Vec<String>, Error> {
        let client = self.client();

        let query_res = client.exec(query).unwrap();
        let cids: Vec<_> = query_res.rows().map(|row| row.get_n(0).unwrap()).collect();

        Ok(cids)
    }
}
