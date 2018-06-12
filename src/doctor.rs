use tokio_core;
use web3;
use std::collections::HashMap;
use web3::types::H160;
use rustc_hex::FromHex;
use console::{style, Emoji};
use failure::{err_msg, Error};

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
    deploy_check_res: Result<bool, Error>,
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

/// Check if all known contracts of the Rlay protocol have been properly deployed
pub fn check_contracts(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<web3::transports::WebSocket>,
) {
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

    let mut contract_addresses: HashMap<&str, String> = HashMap::new();
    contract_addresses.insert(
        "OntologyStorage",
        "0xf0cd575450fc03b90eead03d65e79741a19665e4".to_owned(),
    );
    contract_addresses.insert(
        "RlayToken",
        "0x10ef71366ad76d6bddddc66677c38e137aa564db".to_owned(),
    );
    contract_addresses.insert(
        "PropositionLedger",
        "0xe90f43a68756d880f2dc83e0ae1bf51d31726d36".to_owned(),
    );

    let mut contract_deployed: HashMap<&str, Result<bool, Error>> = HashMap::new();
    for (name, bytecode) in contract_bytecodes {
        let address_bytes = contract_addresses[name][2..].from_hex().unwrap();
        let address_hash: H160 = H160::from_slice(&address_bytes);
        let is_deployed = check_address_code(eloop, &web3, address_hash, bytecode);
        contract_deployed.insert(name, is_deployed);
    }

    println!("Checking contracts:");
    for (name, is_deployed) in contract_deployed.into_iter() {
        print_contract_check(name, &contract_addresses[name], is_deployed);
    }
}

pub fn run_checks() {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();
    let web3 = web3::Web3::new(
        web3::transports::WebSocket::with_event_loop("ws://localhost:8545", &eloop.handle())
            .unwrap(),
    );

    check_contracts(&mut eloop, &web3);
}
