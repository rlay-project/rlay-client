use failure::err_msg;
use redis::FromRedisValue;
use redis::Value as RedisValue;
use rlay_ontology::prelude::*;
use serde_json::Value as JsonValue;

type StdError = Box<dyn std::error::Error>;

#[derive(Debug)]
pub struct GetQueryRelationship {
    pub n_id: u64,
    n_value: JsonValue,
    relationship: String,
    m_cid: String,
}

impl GetQueryRelationship {
    pub fn parse(rel_result: RedisValue) -> Result<Self, StdError> {
        let val_inner = Vec::<RedisValue>::from_redis_value(&rel_result)?;

        Ok(Self {
            n_id: Self::parse_n_id(&val_inner[0])?,
            relationship: Self::parse_relationship(&val_inner[1])?,
            m_cid: Self::parse_m_cid(&val_inner[2])?,
            n_value: Self::parse_n_value(&val_inner[0])?,
        })
    }

    fn parse_kv_pairs(vals: Vec<RedisValue>) -> Result<Vec<(String, RedisValue)>, StdError> {
        vals.iter()
            .map(|kv_pair| {
                let key_value = Vec::<RedisValue>::from_redis_value(kv_pair)?;
                let key = String::from_redis_value(&key_value[0])?;
                let value = key_value[1].clone();

                Ok((key, value))
            })
            .collect()
    }

    fn map_kv_pairs(pairs: Vec<(String, RedisValue)>) -> Vec<(String, JsonValue)> {
        pairs
            .into_iter()
            .filter_map(|(key, redis_value)| match redis_value {
                RedisValue::Data(_) => {
                    let str_val = String::from_redis_value(&redis_value).ok();
                    match str_val {
                        Some(str_val) => Some((key, JsonValue::String(str_val))),
                        None => None,
                    }
                }
                _ => None,
            })
            .collect()
    }

    fn parse_n_id(val: &RedisValue) -> Result<u64, StdError> {
        let val_inner = Vec::<RedisValue>::from_redis_value(val)?;
        let kv_pairs = Self::parse_kv_pairs(val_inner)?;
        let wrapped_id = kv_pairs
            .iter()
            .find(|(key, _)| key == "id")
            .ok_or_else(|| err_msg("No id value found in query result"))?;
        let id = u64::from_redis_value(&wrapped_id.1)?;

        Ok(id)
    }

    fn parse_n_value(val: &RedisValue) -> Result<JsonValue, StdError> {
        let val_inner = Vec::<RedisValue>::from_redis_value(val)?;
        let kv_pairs = Self::parse_kv_pairs(val_inner)?;
        let properties = kv_pairs
            .into_iter()
            .find(|(key, _)| key == "properties")
            .ok_or_else(|| err_msg("No id value found in query result"))?
            .1;
        let properties = Vec::<RedisValue>::from_redis_value(&properties)?;
        let properties_kv_pairs = Self::parse_kv_pairs(properties)?;
        let json_kv_pairs: Vec<_> = Self::map_kv_pairs(properties_kv_pairs);

        let json_object: serde_json::Map<_, _> = json_kv_pairs.into_iter().collect();

        Ok(JsonValue::Object(json_object))
    }

    fn parse_relationship(val: &RedisValue) -> Result<String, StdError> {
        Ok(String::from_redis_value(val)?)
    }

    fn parse_m_cid(val: &RedisValue) -> Result<String, StdError> {
        let val_inner = Vec::<RedisValue>::from_redis_value(val)?;
        let kv_pairs = Self::parse_kv_pairs(val_inner)?;
        let properties = kv_pairs
            .into_iter()
            .find(|(key, _)| key == "properties")
            .ok_or_else(|| err_msg("No id value found in query result"))?
            .1;
        let properties = Vec::<RedisValue>::from_redis_value(&properties)?;
        let properties_kv_pairs = Self::parse_kv_pairs(properties)?;
        let json_kv_pairs: Vec<_> = Self::map_kv_pairs(properties_kv_pairs);

        let cid = json_kv_pairs
            .into_iter()
            .find(|(key, _)| key == "cid")
            .ok_or_else(|| err_msg("No cid property found in query result"))?
            .1;

        Ok(cid
            .as_str()
            .ok_or_else(|| err_msg("cid property is not a string"))?
            .to_owned())
    }

    /// Construct a single Entity from all the relationships
    pub fn merge_into_entity(relationships: Vec<Self>) -> Result<Option<Entity>, StdError> {
        trace!("Relationships to merge into entity: {:?}", &relationships);
        if relationships.len() == 0 {
            return Ok(None);
        }
        let mut entity = relationships
            .first()
            .unwrap()
            .n_value
            .as_object()
            .unwrap()
            .clone();

        // build empty entity with which we can check if fields are supposed to be arrays
        let entity_kind = EntityKind::from_name(entity["type"].clone().as_str().unwrap()).unwrap();
        let empty_entity: JsonValue =
            serde_json::to_value(FormatWeb3(entity_kind.empty_entity())).unwrap();
        let is_array_key = |key: &str| match empty_entity.get(key) {
            Some(JsonValue::Array(_)) => Some(true),
            Some(JsonValue::String(_)) => Some(false),
            Some(JsonValue::Null) => Some(false),
            None => None,
            _ => unimplemented!(),
        };

        for relationship in relationships {
            let rel_type = &relationship.relationship;
            match is_array_key(rel_type) {
                None => {
                    continue;
                }
                Some(true) => {
                    if !entity.contains_key(rel_type) {
                        entity.insert(rel_type.clone(), JsonValue::Array(Vec::new()));
                    }
                    entity[rel_type]
                        .as_array_mut()
                        .unwrap()
                        .push(JsonValue::String(relationship.m_cid));
                }
                Some(false) => {
                    entity.insert(rel_type.clone(), JsonValue::String(relationship.m_cid));
                }
            }
        }

        let web3_entity: FormatWeb3<Entity> =
            serde_json::from_value(JsonValue::Object(entity)).unwrap();
        Ok(Some(web3_entity.0))
    }
}
