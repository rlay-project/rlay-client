use console::{style, Emoji};
use failure::Error;
use futures_timer::FutureExt;
use rlay_backend_ethereum::config::EthereumBackendConfig;
use rlay_backend_ethereum::doctor::check_contracts;
use std::time::Duration;
use tokio_core;
use web3;
use web3::Transport;

use crate::config::{BackendConfig, Config};

pub static SUCCESS: Emoji = Emoji("✅  ", "");
pub static FAILURE: Emoji = Emoji("❌  ", "");

fn print_success<A: AsRef<str> + std::fmt::Display, B: AsRef<str> + std::fmt::Display>(
    bold_message: A,
    additional_message: B,
) {
    print!("  ");
    println!(
        "{}{} ({})",
        SUCCESS,
        style(bold_message).green(),
        additional_message
    )
}

fn print_failure<A: AsRef<str> + std::fmt::Display, B: AsRef<str> + std::fmt::Display>(
    bold_message: A,
    additional_message: B,
) {
    print!("  ");
    println!(
        "{}{} ({})",
        FAILURE,
        style(bold_message).green(),
        additional_message
    )
}

pub fn print_contract_check(
    contract_name: &str,
    address: &str,
    deploy_check_res: &Result<bool, Error>,
) {
    match deploy_check_res {
        Ok(true) => print_success(
            format!("{} deployed", contract_name),
            format!("at {}", address),
        ),
        Ok(false) | Err(_) => print_success(
            format!("{} not deployed", contract_name),
            format!("looking at {}", address),
        ),
    }
}

/// Check deployment of Ethereum contracts via `check_contracts` and print output for doctor CLI.
pub fn check_contracts_print(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<impl Transport>,
    config: &EthereumBackendConfig,
) {
    if config.contract_addresses.is_empty() {
        println!(
            "{}",
            style("Skipping contracts check (missing contract_addresses config)").yellow()
        );
        return;
    }

    let contract_matches_abi = check_contracts(eloop, web3, config);

    println!("Checking contract ABIs:");
    for (name, matches_abi) in contract_matches_abi {
        print_contract_check(&name, &config.contract_addresses[&name], &matches_abi);
    }
}

/// Check connection with Web3 JSON-RPC provider.
pub fn check_web3(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<impl Transport>,
    config: &EthereumBackendConfig,
) {
    let version_future = web3.net().version().timeout(Duration::from_secs(10));

    println!("Checking Web3 JSON-RPC connection:");
    match eloop.run(version_future) {
        Ok(_) => print_success(
            "Able to connect to JSON-RPC",
            format!("at \"{}\"", config.network_address.as_ref().unwrap()),
        ),
        Err(_) => print_failure(
            "Unable to connect to JSON-RPC after 10s timeout",
            format!("at \"{}\"", config.network_address.as_ref().unwrap()),
        ),
    }
}

pub fn run_checks_backend_ethereum(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<impl Transport>,
    name: &str,
    config: &EthereumBackendConfig,
) {
    println!("Checking backend \"{}\":", name);
    check_web3(eloop, web3, config);
    check_contracts_print(eloop, web3, config);
}

pub fn run_doctor(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();
    let web3 = config.web3_with_handle(&eloop.handle());

    for (backend_name, backend_config) in config.backends.iter() {
        match backend_config {
            BackendConfig::Ethereum(config) => {
                run_checks_backend_ethereum(&mut eloop, &web3, backend_name, config);
            }
            #[cfg(feature = "backend_neo4j")]
            BackendConfig::Neo4j(_) => {}
        }
    }
}
