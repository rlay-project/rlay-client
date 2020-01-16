use hyper::{header, Body, Client, Request};
use serde_json::Value;

use super::JsonRpcResult;

pub async fn proxy_rpc_call(target_url: String, request_body: Value) -> JsonRpcResult<Value> {
    let client = Client::new();
    let req = Request::builder()
        .method("POST")
        .uri(target_url)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(request_body.to_string()))
        .expect("request builder");

    let res = client.request(req).await.unwrap();
    let body = hyper::body::to_bytes(res).await.unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();

    Ok(value)
}
