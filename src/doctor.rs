use console::{style, Emoji};
use failure::{err_msg, Error};
use rustc_hex::FromHex;
use std::collections::HashMap;
use std::time::Duration;
use tokio_core;
use web3::types::H160;
use web3;
use futures_timer::FutureExt;

use config::Config;

pub static SUCCESS: Emoji = Emoji("✅  ", "");
pub static FAILURE: Emoji = Emoji("❌  ", "");

/// Check if the contract code at the address is what we expect it to be
pub fn check_address_code(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<web3::transports::WebSocket>,
    address: H160,
    bytecode: &str,
) -> Result<bool, Error> {
    let ontology_code = eloop
        .run(web3.eth().code(address, None))
        .map_err(|_| err_msg("Failed to fetch contract code"))?;
    let content_bytes = bytecode[2..].from_hex().unwrap();

    let bytecode_equal = ontology_code.0 == content_bytes;

    Ok(bytecode_equal)
}

pub fn print_contract_check(
    contract_name: &str,
    address: &str,
    deploy_check_res: &Result<bool, Error>,
) {
    print!("  ");
    match deploy_check_res {
        Ok(true) => println!(
            "{}{} (at {})",
            SUCCESS,
            style(format!("{} deployed", contract_name)).green(),
            address
        ),
        Ok(false) | Err(_) => println!(
            "{}{} (looking at {})",
            FAILURE,
            style(format!("{} not deployed", contract_name)).red(),
            address
        ),
    }
}

/// Check if all known contracts of the Rlay protocol have been properly deployed.
pub fn check_contracts(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<web3::transports::WebSocket>,
    config: &Config,
) {
    if config.contract_addresses.is_empty() {
        println!(
            "{}",
            style("Skipping contracts check (missing contract_addresses config)").yellow()
        );
        return;
    }

    let mut contract_bytecodes = HashMap::new();
    contract_bytecodes.insert(
        "OntologyStorage",
        include_str!("../data/OntologyStorage.bin"),
    );
    contract_bytecodes.insert("RlayToken", include_str!("../data/RlayToken.bin"));
    contract_bytecodes.insert(
        "PropositionLedger",
        include_str!("../data/PropositionLedger.bin"),
    );

    let mut contract_deployed: HashMap<&str, Result<bool, Error>> = HashMap::new();
    for (name, bytecode) in contract_bytecodes {
        let address_hash = config.contract_address(name);
        let is_deployed = check_address_code(eloop, &web3, address_hash, bytecode);
        contract_deployed.insert(name, is_deployed);
    }

    println!("Checking contracts:");
    for (name, is_deployed) in contract_deployed {
        print_contract_check(name, &config.contract_addresses[name], &is_deployed);
    }
}

/// Check connection with Web3 JSON-RPC provider.
pub fn check_web3(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<web3::transports::WebSocket>,
    config: &Config,
) {
    let version_future = web3.net().version().timeout(Duration::from_secs(10));

    println!("Checking Web3 JSON-RPC connection:");
    print!("  ");
    match eloop.run(version_future) {
        Ok(_) => println!(
            "{}{} (at \"{}\")",
            SUCCESS,
            style("Able to connect to JSON-RPC").green(),
            config.network_address.as_ref().unwrap()
        ),
        Err(_) => println!(
            "{}{} (at \"{}\")",
            FAILURE,
            style("Unable to connect to JSON-RPC after 10s timeout").red(),
            config.network_address.as_ref().unwrap()
        ),
    }
}

pub fn run_checks(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();
    let web3 = web3::Web3::new(
        web3::transports::WebSocket::with_event_loop(
            config.network_address.as_ref().unwrap(),
            &eloop.handle(),
        ).unwrap(),
    );

    check_web3(&mut eloop, &web3, &config);
    check_contracts(&mut eloop, &web3, &config);
}
