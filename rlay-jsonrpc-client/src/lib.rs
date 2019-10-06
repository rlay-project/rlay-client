#[macro_use]
extern crate serde_json;

use futures::prelude::*;
use hyper::{client::HttpConnector, header, Body, Client, Request};
use rlay_ontology::ontology::Entity;
use rlay_ontology::prelude::FormatWeb3;
use rlay_resolve::{BoxFuture, ResolveCid};
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
        let body = res.into_body().try_concat().await.unwrap();
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

impl<'a> ResolveCid<'a> for RlayClient {
    type F = BoxFuture<'a, Option<Entity>>;

    fn resolve<B: AsRef<[u8]>>(&'a self, cid: B) -> Self::F {
        let cid: String = serde_json::to_string(&FormatWeb3(cid.as_ref().to_vec())).unwrap();
        // remove quotes from serialized string
        let cid2 = cid[1..cid.len() - 1].to_owned();

        self.get_entity(cid2).unwrap_or_else(|_| None).boxed()
    }
}
