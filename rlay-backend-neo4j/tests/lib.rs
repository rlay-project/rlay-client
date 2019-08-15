use rlay_backend::BackendRpcMethods;
use rlay_backend_neo4j::*;
use rlay_ontology::prelude::*;
use rustc_hex::{FromHex, ToHex};
use serde_json::Value;
use testcontainers::*;
use tokio::runtime::Runtime;

fn neo4j_container() -> images::generic::GenericImage {
    images::generic::GenericImage::new("neo4j:3.4.8")
        .with_wait_for(images::generic::WaitFor::message_on_stdout(
            "Remote interface available at",
        ))
        .with_env_var("NEO4J_AUTH", "none")
}

#[test]
fn store_entity_returns_correct_cid() {
    let _ = env_logger::try_init();
    let rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(neo4j_container());

    let connection_string = format!(
        "http://127.0.0.1:{}/db/data/",
        node.get_host_port(7474).unwrap()
    );

    let backend_config = config::Neo4jBackendConfig {
        uri: connection_string,
    };
    let mut backend = Neo4jBackend::from_config(backend_config);

    let insert_cid = rt
        .block_on(backend.store_entity(&Annotation::default().into(), &Value::Null))
        .unwrap();
    let expected_cid: Vec<u8> =
        "019580031b2088868a58d3aac6d2558a29b3b8cacf3c9788364f57a3470158283121a15dcae0"
            .from_hex()
            .unwrap();

    assert_eq!(expected_cid, insert_cid.to_bytes());
}

#[test]
fn store_and_get_roundtrip_works() {
    let _ = env_logger::try_init();
    let rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(neo4j_container());

    let connection_string = format!(
        "http://127.0.0.1:{}/db/data/",
        node.get_host_port(7474).unwrap()
    );

    let backend_config = config::Neo4jBackendConfig {
        uri: connection_string,
    };
    let mut backend = Neo4jBackend::from_config(backend_config);

    let inserted_entity = Annotation::default().into();
    let inserted_cid = rt
        .block_on(backend.store_entity(&inserted_entity, &Value::Null))
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
    let node = docker.run(neo4j_container());

    let connection_string = format!(
        "http://127.0.0.1:{}/db/data/",
        node.get_host_port(7474).unwrap()
    );

    let backend_config = config::Neo4jBackendConfig {
        uri: connection_string,
    };
    let mut backend = Neo4jBackend::from_config(backend_config);

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
