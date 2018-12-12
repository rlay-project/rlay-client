use console::{style, Emoji};
use ethabi;
use failure::{err_msg, Error};
use futures_timer::FutureExt;
use rustc_hex::FromHex;
use std::collections::HashMap;
use std::time::Duration;
use tokio_core;
use web3;
use web3::types::H160;
use web3::Transport;

use crate::config::{Config, EthereumBackendConfig};

pub static SUCCESS: Emoji = Emoji("✅  ", "");
pub static FAILURE: Emoji = Emoji("❌  ", "");

/// Check if the contract code at the address is what we expect it to be
pub fn check_address_code(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<impl Transport>,
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

fn input_param_types(function: &ethabi::Function) -> Vec<ethabi::ParamType> {
    function.inputs.iter().map(|p| p.kind.clone()).collect()
}

fn short_signature(name: &str, params: &[ethabi::ParamType]) -> [u8; 4] {
    let mut result = [0u8; 4];
    fill_signature(name, params, &mut result);
    result
}

fn fill_signature(name: &str, params: &[ethabi::ParamType], result: &mut [u8]) {
    let types = params
        .iter()
        .map(ethabi::param_type::Writer::write)
        .collect::<Vec<String>>()
        .join(",");

    let data: Vec<u8> = From::from(format!("{}({})", name, types).as_str());

    let mut sponge = ::tiny_keccak::Keccak::new_keccak256();
    sponge.update(&data);
    sponge.finalize(result);
}

fn function_signature(function: &ethabi::Function) -> ethabi::Result<[u8; 4]> {
    let params = input_param_types(function);

    Ok(short_signature(&function.name, &params))
}

pub fn check_address_abi(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<impl Transport>,
    address: H160,
    abi: &str,
) -> Result<bool, Error> {
    let address_code = eloop
        .run(web3.eth().code(address, None))
        .map_err(|_| err_msg("Failed to fetch contract code"))?;
    // println!("ADDRESS CODE: {:?}", address_code.0.to_hex());

    let contract = ethabi::Contract::load(abi.as_bytes()).unwrap();
    for function in contract.functions() {
        let signature = function_signature(function).unwrap();
        let position = address_code
            .0
            .windows(signature.to_vec().len())
            .position(|window| window == signature.to_vec().as_slice());
        if position.is_none() {
            return Ok(false);
        }
    }

    Ok(true)
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

    let mut contract_abis = HashMap::new();
    contract_abis.insert(
        "OntologyStorage",
        include_str!("../data/OntologyStorage.abi"),
    );
    contract_abis.insert("RlayToken", include_str!("../data/RlayToken.abi"));
    contract_abis.insert(
        "PropositionLedger",
        include_str!("../data/PropositionLedger.abi"),
    );

    let mut contract_matches_abi: HashMap<&str, Result<bool, Error>> = HashMap::new();
    for (name, abi) in contract_abis {
        let address_hash = config.contract_address(name);
        let matches_abi = check_address_abi(eloop, &web3, address_hash, abi);
        contract_matches_abi.insert(name, matches_abi);
    }

    println!("Checking contract ABIs:");
    for (name, matches_abi) in contract_matches_abi {
        print_contract_check(name, &config.contract_addresses[name], &matches_abi);
    }
}

/// Check connection with Web3 JSON-RPC provider.
pub fn check_web3(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<impl Transport>,
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
            config
                .default_eth_backend_config()
                .unwrap()
                .network_address
                .as_ref()
                .unwrap()
        ),
        Err(_) => println!(
            "{}{} (at \"{}\")",
            FAILURE,
            style("Unable to connect to JSON-RPC after 10s timeout").red(),
            config
                .default_eth_backend_config()
                .unwrap()
                .network_address
                .as_ref()
                .unwrap()
        ),
    }
}

pub fn run_checks(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();
    let web3 = config.web3_with_handle(&eloop.handle());

    check_web3(&mut eloop, &web3, &config);
    check_contracts(
        &mut eloop,
        &web3,
        &config.default_eth_backend_config().unwrap(),
    );
}
