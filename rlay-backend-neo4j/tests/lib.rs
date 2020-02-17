use lazy_static::lazy_static;
use nonparallel::nonparallel;
use rlay_backend::rpc::*;
use rlay_backend::GetEntity;
use rlay_backend_neo4j::*;
use rlay_ontology::prelude::*;
use rustc_hex::{FromHex, ToHex};
use serde_json::Value;
use std::sync::Mutex;
use testcontainers::*;
use tokio::runtime::Runtime;

lazy_static! {
    static ref MUT_A: Mutex<()> = Mutex::new(());
}

fn neo4j_container() -> images::generic::GenericImage {
    images::generic::GenericImage::new("neo4j:3.4.8")
        .with_wait_for(images::generic::WaitFor::message_on_stdout(
            "Remote interface available at",
        ))
        .with_env_var("NEO4J_AUTH", "none")
}

#[test]
#[nonparallel(MUT_A)]
fn store_entity_returns_correct_cid() {
    let _ = env_logger::try_init();
    let mut rt = Runtime::new().unwrap();
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
#[nonparallel(MUT_A)]
fn store_and_get_roundtrip_works() {
    let _ = env_logger::try_init();
    let mut rt = Runtime::new().unwrap();
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
        .block_on(BackendRpcMethodGetEntity::get_entity(
            &mut backend,
            &formatted_cid,
        ))
        .unwrap()
        .unwrap();

    assert_eq!(
        inserted_entity, retrieved_entity,
        "inserted and retrieved entity don't match"
    );

    let retrieved_entity = rt
        .block_on(GetEntity::get_entity(&backend, &inserted_cid.to_bytes()))
        .unwrap()
        .unwrap();

    assert_eq!(
        inserted_entity, retrieved_entity,
        "inserted and retrieved entity don't match"
    );
}

#[test]
#[nonparallel(MUT_A)]
fn resolve_entity_works() {
    let _ = env_logger::try_init();
    let mut rt = Runtime::new().unwrap();
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

    let ind = Individual::default().into();
    let ind_cid = rt
        .block_on(backend.store_entity(&ind, &Value::Null))
        .unwrap();
    let formatted_cid: String = format!("0x{}", ind_cid.to_bytes().to_hex());

    let dpa = DataPropertyAssertion {
        subject: Some(ind_cid.to_bytes()),
        property: Some(vec![12, 34]),
        target: Some(vec![56, 78]),
        ..DataPropertyAssertion::default()
    }
    .into();
    let dpa_cid = rt
        .block_on(backend.store_entity(&dpa, &Value::Null))
        .unwrap();
    let dpa_cid_formatted: String = format!("0x{}", dpa_cid.to_bytes().to_hex());

    let resolved_entities = rt.block_on(backend.resolve_entity(&formatted_cid)).unwrap();

    dbg!(&formatted_cid);
    dbg!(&dpa_cid_formatted);
    dbg!(&resolved_entities);
    assert_eq!(
        resolved_entities.get(&formatted_cid).unwrap(),
        &vec![ind, dpa],
        "inserted and retrieved entity don't match"
    );
}

#[test]
#[nonparallel(MUT_A)]
/// When using a CID in an entity, a leaf node is created in the graph, that doesn't have enough
/// information on it to be correctly restored, so it should be non-retrievable like a CID that is
/// unknown.
fn get_entity_leaf_node_returns_none() {
    let _ = env_logger::try_init();
    let mut rt = Runtime::new().unwrap();
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
        .block_on(BackendRpcMethodGetEntity::get_entity(
            &mut backend,
            "019580031b201111111111111111111111111111111111111111111111111111111111111111",
        ))
        .unwrap();

    assert!(retrieved_entity.is_none());
}
