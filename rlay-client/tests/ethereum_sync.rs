use assert_cmd::prelude::*;
use rand::Rng;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;
use testcontainers::*;

fn ganache_container() -> images::generic::GenericImage {
    images::generic::GenericImage::new("trufflesuite/ganache-cli:v6.1.0")
        .with_args(vec!["--seed".to_owned(), "1234".to_owned()])
        .with_wait_for(images::generic::WaitFor::message_on_stdout(
            "Listening on localhost:8545",
        ))
}

fn set_ganache_port(path: &Path, port: u32) {
    let contents = std::fs::read_to_string(path).unwrap();
    let new_contents = contents.replace("<GANACHE_PORT>", &port.to_string());
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
fn setup_deploy() {
    let config_file = NamedTempFile::new().unwrap();
    std::fs::copy("./tests/rlay.config.toml.test_template", config_file.path()).unwrap();

    let docker = clients::Cli::default();
    let ganache_node = docker.run(ganache_container());
    set_ganache_port(
        config_file.path(),
        ganache_node.get_host_port(8545).unwrap().into(),
    );
    set_rpc_port(config_file.path());

    let output = Command::cargo_bin("rlay-client")
        .unwrap()
        .args(&[
            "deploy-contracts",
            "--from",
            "0xc02345a911471fd46c47c4d3c2e5c85f5ae93d13",
            "--config",
            config_file.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    println!("");
    println!("STDOUT {}", std::str::from_utf8(&output.stdout).unwrap());
    println!("STDERR {}", std::str::from_utf8(&output.stderr).unwrap());
}
