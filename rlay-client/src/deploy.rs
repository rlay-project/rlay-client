use futures::compat::Compat;
use futures::prelude::*;
use rlay_backend_ethereum::deploy::deploy_contracts;
use rustc_hex::ToHex;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use toml_edit::{value, Document};

use crate::config::Config;

pub fn deploy_contracts_with_log(config: &Config, deployer_address: &str) {
    let mut rt = tokio_core::reactor::Core::new().unwrap();
    let web3 = config.web3_with_handle(&rt.handle());

    let mut deployed_addresses = BTreeMap::new();
    let library_addresses_fut = deploy_contracts(&web3, deployer_address)
        .inspect(|(contract_name, deployed_address)| {
            deployed_addresses.insert(contract_name.clone(), deployed_address.clone());
            info!(
                "Deployed {contract} at {address}",
                contract = contract_name,
                address = deployed_address.to_hex(),
            );
        })
        .collect::<Vec<_>>();
    rt.run(Compat::new(library_addresses_fut.unit_error()))
        .unwrap();

    let rlay_token_address = deployed_addresses.get("RlayToken").unwrap();
    let ontology_storage_address = deployed_addresses.get("OntologyStorage").unwrap();
    let proposition_ledger_address = deployed_addresses.get("PropositionLedger").unwrap();

    if let Some(ref config_path) = config.config_path {
        let contents = {
            let mut file = File::open(config_path).unwrap();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            contents
        };

        let mut doc = contents.parse::<Document>().expect("invalid doc");

        doc["backends"]["default_eth"]["contract_addresses"]["RlayToken"] =
            value(format!("0x{}", rlay_token_address.to_hex()));
        doc["backends"]["default_eth"]["contract_addresses"]["OntologyStorage"] =
            value(format!("0x{}", ontology_storage_address.to_hex()));
        doc["backends"]["default_eth"]["contract_addresses"]["PropositionLedger"] =
            value(format!("0x{}", proposition_ledger_address.to_hex()));

        let mut file = File::create(config_path).unwrap();
        file.write_all(doc.to_string().as_bytes()).unwrap();
    }
}
