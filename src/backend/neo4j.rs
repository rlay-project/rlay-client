use cid::{Cid, ToCid};
#[allow(unused_imports)]
use failure::{err_msg, Error};
use rlay_ontology::prelude::*;
use rustc_hex::ToHex;
use rusted_cypher::GraphClient;
use serde_json::{self, Value};

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
        let entity_val = serde_json::to_value(entity.clone().to_web3_format()).unwrap();
        let val = entity_val.as_object().unwrap();
        let mut values = Vec::new();
        let mut relationships = Vec::new();
        {
            let mut add_relationship_value = |cid, key, value| {
                let rel_query = format!(
                    "MATCH (n {{ cid: \"{0}\"}}) MERGE (m {{ cid: {2} }}) MERGE (n)-[r:{1}]->(m)",
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

        let mut query = format!("MERGE (n {{cid: \"{1}\"}}) SET n:{0}", kind_name, cid);
        if !values.is_empty() {
            query.push_str(", ");
            query.push_str(&values.join(", "));
        }

        trace!("NEO4J QUERY: {}", query);
        client.exec(query).unwrap();
        for relationship in relationships {
            trace!("NEO4J QUERY: {}", relationship);
            client.exec(relationship).unwrap();
        }

        Ok(raw_cid)
    }

    fn get_entity(&mut self, cid: &str) -> Result<Option<Entity>, Error> {
        let client = self.client();

        let query = format!(
            "MATCH (n {{ cid: \"{0}\"}})-[r]->(m) RETURN labels(n),n,type(r),m",
            cid
        );
        trace!("get_entity query: {:?}", query);
        let query_res = client.exec(query).unwrap();
        if query_res.rows().count() == 0 {
            return Ok(None);
        }

        let first_row = query_res.rows().next().unwrap();
        let labels: Value = first_row.get("labels(n)").unwrap();
        let label = labels.as_array().unwrap()[0].clone();
        // build empty entity with which we can check if fields are supposed to be arrays
        let entity_kind = EntityKind::from_name(label.as_str().unwrap()).unwrap();
        let empty_entity: Value =
            serde_json::to_value(entity_kind.empty_entity().to_web3_format()).unwrap();

        let mut entity: Value = first_row.get("n").unwrap();
        entity["type"] = label;
        entity.as_object_mut().unwrap().remove("cid");

        for row in query_res.rows() {
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
                _ => unimplemented!(),
            }
        }

        let web3_entity: EntityFormatWeb3 = serde_json::from_value(entity).unwrap();
        let entity: Entity = Entity::from_web3_format(web3_entity);

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
