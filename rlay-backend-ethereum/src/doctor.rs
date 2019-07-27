use ethabi;
use failure::{err_msg, Error};
use std::collections::HashMap;
use tokio_core;
use web3;
use web3::types::H160;
use web3::Transport;

use crate::config::EthereumBackendConfig;

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

/// Check if all known contracts of the Rlay protocol have been properly deployed.
pub fn check_contracts(
    eloop: &mut tokio_core::reactor::Core,
    web3: &web3::Web3<impl Transport>,
    config: &EthereumBackendConfig,
) -> HashMap<String, Result<bool, Error>> {
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

    let mut contract_matches_abi: HashMap<String, Result<bool, Error>> = HashMap::new();
    for (name, abi) in contract_abis {
        let address_hash = config.contract_address(name);
        let matches_abi = check_address_abi(eloop, &web3, address_hash, abi);
        contract_matches_abi.insert(name.to_owned(), matches_abi);
    }

    contract_matches_abi
}
