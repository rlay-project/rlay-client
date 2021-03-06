#[macro_use]
extern crate serde_json;

use async_trait::async_trait;
use failure::err_msg;
use futures::prelude::*;
use hyper::{client::HttpConnector, header, Body, Client, Request};
use rlay_backend::GetEntity;
use rlay_ontology::ontology::Entity;
use rlay_ontology::prelude::FormatWeb3;
use rustc_hex::ToHex;
use serde_json::Map;
use serde_json::Value;

#[derive(Clone)]
pub struct RlayClient {
    client: Client<HttpConnector, Body>,
    base_url: String,
}

impl RlayClient {
    pub fn new(url: &str) -> Self {
        let client = Client::new();

        Self {
            client,
            base_url: url.to_owned(),
        }
    }

    async fn call_method(&self, method_name: &str, params: Value) -> Result<Value, ()> {
        let req = Request::builder()
            .method("POST")
            .uri(self.base_url.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json! {{
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": method_name,
                    "params": params,
                }}
                .to_string(),
            ))
            .expect("request builder");

        let res = self.client.request(req).await.unwrap();
        let body = hyper::body::to_bytes(res).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();

        Ok(value)
    }

    pub async fn version(&self) -> Result<Map<String, Value>, ()> {
        let res = self
            .call_method("rlay_version", json! {null})
            .await
            .unwrap();
        let value = res["result"].as_object().unwrap().to_owned();

        Ok(value)
    }

    pub async fn get_entity<C: AsRef<str> + serde::ser::Serialize>(
        &self,
        cid: C,
    ) -> Result<Option<Entity>, ()> {
        let res = self
            .call_method("rlay_experimentalGetEntity", json! {[cid]})
            .await
            .unwrap();
        let value = res["result"].clone();
        match value {
            Value::Null => Ok(None),
            Value::Object(obj) => {
                let value_obj = obj.to_owned();
                let value = Value::Object(value_obj);

                let entity: FormatWeb3<_> = serde_json::from_value(value).unwrap();
                Ok(Some(entity.0))
            }
            _ => Err(()),
        }
    }

    pub async fn store_entity<E: Into<Entity>>(&self, entity: E) -> Result<String, ()> {
        let res = self
            .call_method(
                "rlay_experimentalStoreEntity",
                json! {[FormatWeb3(entity.into())]},
            )
            .await
            .unwrap();

        let value = res["result"].clone();
        match value {
            Value::String(inner) => Ok(inner.to_owned()),
            _ => Err(()),
        }
    }
}

#[async_trait]
impl GetEntity for RlayClient {
    async fn get_entity(&self, cid: &[u8]) -> Result<Option<Entity>, rlay_backend::Error> {
        let cid_str: String = cid.as_ref().to_hex();

        self.get_entity(cid_str)
            .map_err(|_| err_msg("Failure during RPC call"))
            .await
    }
}
