#![recursion_limit = "128"]
#![feature(async_await)]

use redis::FromRedisValue;
use rlay_backend::BackendRpcMethods;
use rlay_backend_redisgraph::*;
use rlay_ontology::prelude::*;
use rustc_hex::{FromHex, ToHex};
use serde_json::Value;
use testcontainers::*;
use tokio::runtime::Runtime;

fn redis_container() -> images::generic::GenericImage {
    images::generic::GenericImage::new("redislabs/redisgraph@sha256:0b4ee7d857dbfe3d9fed1de79882e65cee56b6fa9747e4ce4619984cb207a56b")
        .with_wait_for(images::generic::WaitFor::message_on_stdout(
            "Ready to accept connections",
        ))
}

#[test]
fn store_entity_returns_correct_cid() {
    let _ = env_logger::try_init();
    let rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(redis_container());

    let connection_string = format!("redis://127.0.0.1:{}", node.get_host_port(6379).unwrap());

    let backend_config = config::RedisgraphBackendConfig {
        uri: connection_string,
        graph_name: "rlaygraph".to_owned(),
    };
    let mut backend = RedisgraphBackend::from_config(backend_config);

    let insert_cid = rt
        .block_on(backend.store_entity(&Annotation::default().into(), &Value::Null))
        .unwrap();
    // let insert_cid: cid::Cid = unimplemented!();
    let expected_cid: Vec<u8> =
        "019580031b2088868a58d3aac6d2558a29b3b8cacf3c9788364f57a3470158283121a15dcae0"
            .from_hex()
            .unwrap();

    assert_eq!(expected_cid, insert_cid.to_bytes());
}

#[test]
fn multiple_store_produces_correct_number_of_nodes() {
    let _ = env_logger::try_init();
    let rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(redis_container());

    let connection_string = format!("redis://127.0.0.1:{}", node.get_host_port(6379).unwrap());

    let backend_config = config::RedisgraphBackendConfig {
        uri: connection_string.clone(),
        graph_name: "rlaygraph".to_owned(),
    };
    let mut backend = RedisgraphBackend::from_config(backend_config);
    let mut backend2 = backend.clone();

    rt.block_on(backend.store_entity(&Annotation::default().into(), &Value::Null))
        .unwrap();
    rt.block_on(backend2.store_entity(&Annotation::default().into(), &Value::Null))
        .unwrap();

    let redis_client = redis::Client::open(connection_string.as_str()).unwrap();
    let count_query_res: Vec<redis::Value> = redis::cmd("GRAPH.QUERY")
        .arg("rlaygraph")
        .arg("MATCH (n:RlayEntity) RETURN COUNT(n)")
        .query(&mut redis_client.get_connection().unwrap())
        .unwrap();
    let count_res = Vec::<redis::Value>::from_redis_value(&count_query_res[1]).unwrap();
    let count_bulk = Vec::<redis::Value>::from_redis_value(&count_res[0]).unwrap();
    let count = u64::from_redis_value(&count_bulk[0]).unwrap();

    assert_eq!(2, count);
}

#[test]
fn store_and_get_roundtrip_works() {
    let _ = env_logger::try_init();
    let rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(redis_container());

    let connection_string = format!("redis://127.0.0.1:{}", node.get_host_port(6379).unwrap());

    let backend_config = config::RedisgraphBackendConfig {
        uri: connection_string,
        graph_name: "rlaygraph".to_owned(),
    };
    let mut backend = RedisgraphBackend::from_config(backend_config);
    let mut backend2 = backend.clone();

    let inserted_entity: Entity = Annotation::default().into();
    let inserted_entity2 = inserted_entity.clone();
    let inserted_cid = rt
        .block_on(backend2.store_entity(&inserted_entity2, &Value::Null))
        .unwrap();
    let formatted_cid: String = format!("0x{}", inserted_cid.to_bytes().to_hex());

    let retrieved_entity = rt
        .block_on(backend.get_entity(&formatted_cid))
        .unwrap()
        .unwrap();

    assert_eq!(
        inserted_entity, retrieved_entity,
        "inserted and retrieved entity don't match"
    );
}

#[test]
/// When using a CID in an entity, a leaf node is created in the graph, that doesn't have enough
/// information on it to be correctly restored, so it should be non-retrievable like a CID that is
/// unknown.
fn get_entity_leaf_node_returns_none() {
    let _ = env_logger::try_init();
    let rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(redis_container());

    let connection_string = format!("redis://127.0.0.1:{}", node.get_host_port(6379).unwrap());

    let backend_config = config::RedisgraphBackendConfig {
        uri: connection_string,
        graph_name: "rlaygraph".to_owned(),
    };
    let mut backend = RedisgraphBackend::from_config(backend_config);

    let mut inserted_ann = Annotation::default();
    let leaf_cid: Vec<u8> =
        "019580031b201111111111111111111111111111111111111111111111111111111111111111"
            .from_hex()
            .unwrap();
    inserted_ann.annotations.push(leaf_cid.clone());
    rt.block_on(backend.store_entity(&inserted_ann.into(), &Value::Null))
        .unwrap();

    let retrieved_entity = rt
        .block_on(backend.get_entity(
            "019580031b201111111111111111111111111111111111111111111111111111111111111111",
        ))
        .unwrap();

    assert!(retrieved_entity.is_none());
}
