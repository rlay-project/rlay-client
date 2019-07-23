use assert_cmd::prelude::*;
use futures01::future::Future;
use std::process::Command;

fn wait_for_docker() {
    loop {
        let (_eloop, transport) = web3::transports::Http::new("http://localhost:9545").unwrap();
        let web3 = web3::Web3::new(transport);
        let version_res = web3.net().version().wait();
        match version_res {
            Ok(_) => {
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::new(1, 0)),
        }
    }
}

#[test]
fn setup() {
    let output = Command::new("docker")
        .args(&["rm", "--force", "rlay-client-ganache"])
        .output()
        .expect("failed to execute process");
    println!("");
    println!("STDOUT {}", std::str::from_utf8(&output.stdout).unwrap());
    println!("STDERR {}", std::str::from_utf8(&output.stderr).unwrap());

    let output = Command::new("docker")
        .args(&[
            "run",
            "-d",
            "--name",
            "rlay-client-ganache",
            "-p",
            "9545:8545",
            "trufflesuite/ganache-cli:v6.1.0",
            "--seed",
            "1234",
        ])
        .output()
        .expect("failed to execute process");
    println!("");
    println!("STDOUT {}", std::str::from_utf8(&output.stdout).unwrap());
    println!("STDERR {}", std::str::from_utf8(&output.stderr).unwrap());
}

#[test]
fn setup_deploy() {
    wait_for_docker();

    std::fs::copy(
        "./tests/rlay.config.toml.test_template",
        "./tests/rlay.config.toml",
    )
    .unwrap();
    let output = Command::main_binary()
        .unwrap()
        .args(&[
            "deploy-contracts",
            "--from",
            "0xc02345a911471fd46c47c4d3c2e5c85f5ae93d13",
            "--config",
            "./tests/rlay.config.toml",
        ])
        .output()
        .unwrap();
    println!("");
    println!("STDOUT {}", std::str::from_utf8(&output.stdout).unwrap());
    println!("STDERR {}", std::str::from_utf8(&output.stderr).unwrap());
}
