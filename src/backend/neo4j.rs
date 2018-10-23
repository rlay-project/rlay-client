#[allow(unused_imports)]
use failure::{err_msg, Error};
use cid::{Cid, ToCid};
use rustc_hex::ToHex;
use rlay_ontology::ontology::Entity;
use serde_json::{self, Value};

use backend::{BackendFromConfig, BackendRpcMethods};
use config::backend::Neo4jBackendConfig;

pub struct Neo4jBackend {
    pub config: Neo4jBackendConfig,
}

impl BackendFromConfig for Neo4jBackend {
    type C = Neo4jBackendConfig;

    fn from_config(config: Self::C) -> Result<Self, Error> {
        Ok(Self { config })
    }
}

impl BackendRpcMethods for Neo4jBackend {
    fn store_entity(&mut self, entity: &Entity, _options_object: &Value) -> Result<Cid, Error> {
        let raw_cid = entity.to_cid().unwrap();
        let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());

        let client = self.config.client().unwrap();
        let kind_name: &str = entity.kind().into();
        let entity_val = serde_json::to_value(entity).unwrap();
        let val = entity_val.as_object().unwrap();
        let mut values = Vec::new();
        let mut relationships = Vec::new();
        // TODO: support non-array relationships
        for (key, value) in val {
            if let Value::Array(array_val) = value {
                for relationship_value in array_val {
                    let rel_query = format!(
                                    "MATCH (n {{ cid: \"{0}\"}}) MERGE (m {{ cid: {2} }}) MERGE (n)-[r:{1}]->(m)",
                                    cid, key, relationship_value
                                );
                    relationships.push(rel_query);
                }
                continue;
            }
            values.push(format!("n.{0} = {1}", key, value));
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
}
