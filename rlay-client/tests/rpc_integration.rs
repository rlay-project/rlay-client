#[macro_use]
extern crate serde_json;

use assert_cmd::prelude::*;
use hyper::Client;
use rand::Rng;
use rlay_jsonrpc_client::RlayClient;
use rlay_ontology::ontology::Annotation;
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;
use testcontainers::*;
use tokio::runtime::Runtime;

fn neo4j_container() -> images::generic::GenericImage {
    images::generic::GenericImage::new("neo4j:3.4.8")
        .with_wait_for(images::generic::WaitFor::message_on_stdout(
            "Remote interface available at",
        ))
        .with_env_var("NEO4J_AUTH", "none")
}

fn set_neo4j_port(path: &Path, port: u32) {
    let contents = std::fs::read_to_string(path).unwrap();
    let new_contents = contents.replace("<NEO4J_PORT>", &port.to_string());
    std::fs::write(path, new_contents).unwrap();
}

fn set_rpc_port(path: &Path) -> u32 {
    let mut rng = rand::thread_rng();
    let port = rng.gen_range(35000, 36000);

    let contents = std::fs::read_to_string(path).unwrap();
    let new_contents = contents.replace("<RPC_PORT>", &port.to_string());
    std::fs::write(path, new_contents).unwrap();

    port
}

#[test]
fn get_health() {
    let _ = env_logger::try_init();
    let config_file = NamedTempFile::new().unwrap();
    std::fs::copy(
        "./tests/rlay.config.neo4j.toml.test_template",
        config_file.path(),
    )
    .unwrap();

    let mut rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(neo4j_container());

    set_neo4j_port(config_file.path(), node.get_host_port(7474).unwrap().into());
    let rpc_port = set_rpc_port(config_file.path());
    let mut child_client = Command::cargo_bin("rlay-client")
        .unwrap()
        .args(&["client", "--config", config_file.path().to_str().unwrap()])
        .spawn()
        .unwrap();

    // HACK: wait for client to start up
    std::thread::sleep(std::time::Duration::new(3, 0));

    let base_url = format!("http://127.0.0.1:{}", rpc_port);
    let rpc_result_value = rt.block_on(async {
        let client = Client::new();
        let res = client
            .get(format!("{}/health", base_url).parse().unwrap())
            .await
            .unwrap();

        let body: Vec<u8> = hyper::body::to_bytes(res).await.unwrap().to_vec();
        let rpc_result_value: Value = serde_json::from_slice(&body).unwrap();
        rpc_result_value
    });

    let expected_value = json!({"status": "healthy"});

    assert_eq!(expected_value, rpc_result_value);

    child_client.kill().unwrap();
}

#[test]
fn store_and_get_roundtrip() {
    let _ = env_logger::try_init();
    let config_file = NamedTempFile::new().unwrap();
    std::fs::copy(
        "./tests/rlay.config.neo4j.toml.test_template",
        config_file.path(),
    )
    .unwrap();

    let mut rt = Runtime::new().unwrap();
    let docker = clients::Cli::default();
    let node = docker.run(neo4j_container());

    set_neo4j_port(config_file.path(), node.get_host_port(7474).unwrap().into());
    let rpc_port = set_rpc_port(config_file.path());
    let mut child_client = Command::cargo_bin("rlay-client")
        .unwrap()
        .args(&["client", "--config", config_file.path().to_str().unwrap()])
        .spawn()
        .unwrap();

    // HACK: wait for client to start up
    std::thread::sleep(std::time::Duration::new(3, 0));

    let client = RlayClient::new(&format!("http://127.0.0.1:{}", rpc_port));

    let cid = rt
        .block_on(client.store_entity(Annotation::default()))
        .unwrap();
    println!("RETURNED CID: {}", cid);

    let retrieved_entity = rt.block_on(client.get_entity(cid)).unwrap();

    assert_eq!(Some(Annotation::default().into()), retrieved_entity);
    child_client.kill().unwrap();
}
