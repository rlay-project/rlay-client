use crate::config::{BackendConfig, Config};

use rustc_hex::FromHex;
use serde_derive::Deserialize;
use std::collections::BTreeMap;
use web3::futures::future::Future;
use web3::types::Address;

fn contract_bins() -> BTreeMap<&'static str, &'static str> {
    let mut bins = BTreeMap::default();
    bins.insert(
        "OntologyStorage",
        include_str!("../data/bins/OntologyStorage.json"),
    );
    bins.insert("RlayToken", include_str!("../data/bins/RlayToken.json"));
    bins.insert(
        "PropositionLedger",
        include_str!("../data/bins/PropositionLedger.json"),
    );

    // LIBRARIES
    bins.insert(
        "ClassStorage",
        include_str!("../data/bins/ClassStorage.json"),
    );
    bins.insert(
        "ObjectIntersectionOfStorage",
        include_str!("../data/bins/ObjectIntersectionOfStorage.json"),
    );
    bins.insert(
        "ObjectUnionOfStorage",
        include_str!("../data/bins/ObjectUnionOfStorage.json"),
    );
    bins.insert(
        "ObjectComplementOfStorage",
        include_str!("../data/bins/ObjectComplementOfStorage.json"),
    );
    bins.insert(
        "ObjectOneOfStorage",
        include_str!("../data/bins/ObjectOneOfStorage.json"),
    );
    bins.insert(
        "ObjectSomeValuesFromStorage",
        include_str!("../data/bins/ObjectSomeValuesFromStorage.json"),
    );
    bins.insert(
        "ObjectAllValuesFromStorage",
        include_str!("../data/bins/ObjectAllValuesFromStorage.json"),
    );
    bins.insert(
        "ObjectHasValueStorage",
        include_str!("../data/bins/ObjectHasValueStorage.json"),
    );
    bins.insert(
        "ObjectHasSelfStorage",
        include_str!("../data/bins/ObjectHasSelfStorage.json"),
    );
    bins.insert(
        "ObjectMinCardinalityStorage",
        include_str!("../data/bins/ObjectMinCardinalityStorage.json"),
    );
    bins.insert(
        "ObjectMaxCardinalityStorage",
        include_str!("../data/bins/ObjectMaxCardinalityStorage.json"),
    );
    bins.insert(
        "ObjectExactCardinalityStorage",
        include_str!("../data/bins/ObjectExactCardinalityStorage.json"),
    );
    bins.insert(
        "DataSomeValuesFromStorage",
        include_str!("../data/bins/DataSomeValuesFromStorage.json"),
    );
    bins.insert(
        "DataAllValuesFromStorage",
        include_str!("../data/bins/DataAllValuesFromStorage.json"),
    );
    bins.insert(
        "DataHasValueStorage",
        include_str!("../data/bins/DataHasValueStorage.json"),
    );
    bins.insert(
        "DataMinCardinalityStorage",
        include_str!("../data/bins/DataMinCardinalityStorage.json"),
    );
    bins.insert(
        "DataMaxCardinalityStorage",
        include_str!("../data/bins/DataMaxCardinalityStorage.json"),
    );
    bins.insert(
        "DataExactCardinalityStorage",
        include_str!("../data/bins/DataExactCardinalityStorage.json"),
    );
    bins.insert(
        "ObjectPropertyStorage",
        include_str!("../data/bins/ObjectPropertyStorage.json"),
    );
    bins.insert(
        "InverseObjectPropertyStorage",
        include_str!("../data/bins/InverseObjectPropertyStorage.json"),
    );
    bins.insert(
        "DataPropertyStorage",
        include_str!("../data/bins/DataPropertyStorage.json"),
    );
    bins.insert(
        "AnnotationStorage",
        include_str!("../data/bins/AnnotationStorage.json"),
    );
    bins.insert(
        "IndividualStorage",
        include_str!("../data/bins/IndividualStorage.json"),
    );
    bins.insert(
        "AnnotationPropertyStorage",
        include_str!("../data/bins/AnnotationPropertyStorage.json"),
    );
    bins.insert(
        "ClassAssertionStorage",
        include_str!("../data/bins/ClassAssertionStorage.json"),
    );
    bins.insert(
        "NegativeClassAssertionStorage",
        include_str!("../data/bins/NegativeClassAssertionStorage.json"),
    );
    bins.insert(
        "ObjectPropertyAssertionStorage",
        include_str!("../data/bins/ObjectPropertyAssertionStorage.json"),
    );
    bins.insert(
        "NegativeObjectPropertyAssertionStorage",
        include_str!("../data/bins/NegativeObjectPropertyAssertionStorage.json"),
    );
    bins.insert(
        "DataPropertyAssertionStorage",
        include_str!("../data/bins/DataPropertyAssertionStorage.json"),
    );
    bins.insert(
        "NegativeDataPropertyAssertionStorage",
        include_str!("../data/bins/NegativeDataPropertyAssertionStorage.json"),
    );
    bins.insert(
        "AnnotationAssertionStorage",
        include_str!("../data/bins/AnnotationAssertionStorage.json"),
    );
    bins.insert(
        "NegativeAnnotationAssertionStorage",
        include_str!("../data/bins/NegativeAnnotationAssertionStorage.json"),
    );

    bins
}

#[derive(Deserialize)]
struct ContractData {
    pub abi: serde_json::Value,
    pub bytecode: web3::types::Bytes,
}

fn deploy_contract<T: web3::contract::tokens::Tokenize + Clone>(
    web3_url: &str,
    contract_name: &str,
    deployer_address: &str,
    constructor_params: T,
) -> (
    web3::transports::EventLoopHandle,
    impl Future<Item = Address, Error = ()>,
) {
    let bins = contract_bins();
    let contract_data: ContractData =
        serde_json::from_str(bins.get(contract_name).unwrap()).expect("Can't read contract data");

    let (_eloop, transport) = web3::transports::WebSocket::new(web3_url).unwrap();
    let web3 = web3::Web3::new(transport);

    let abi = match constructor_params.clone().into_tokens().is_empty() {
        true => serde_json::to_vec(&serde_json::Value::Array(vec![])).unwrap(),
        false => serde_json::to_vec(&contract_data.abi).unwrap(),
    };
    let deploy_contract =
        web3::contract::Contract::deploy(web3.eth(), &abi).expect("Unable to create contract");
    let deployed_contract = deploy_contract
        .options(web3::contract::Options::with(|options| {
            options.gas = Some(web3::types::U256::from(6_000_000));
        }))
        .confirmations(0)
        .execute(
            contract_data.bytecode.0,
            constructor_params,
            web3::types::H160::from_slice(&deployer_address[2..].from_hex().unwrap()),
        )
        .unwrap();

    (
        _eloop,
        deployed_contract
            .and_then(|contract| Ok(contract.address()))
            .map_err(|_| ()),
    )
}

pub fn deploy_contracts(config: &Config, deployer_address: &str) {
    let libraries = vec![
        "Class",
        "ObjectIntersectionOf",
        "ObjectUnionOf",
        "ObjectComplementOf",
        "ObjectOneOf",
        "ObjectSomeValuesFrom",
        "ObjectAllValuesFrom",
        "ObjectHasValue",
        "ObjectHasSelf",
        "ObjectMinCardinality",
        "ObjectMaxCardinality",
        "ObjectExactCardinality",
        "DataSomeValuesFrom",
        "DataAllValuesFrom",
        "DataHasValue",
        "DataMinCardinality",
        "DataMaxCardinality",
        "DataExactCardinality",
        "ObjectProperty",
        "InverseObjectProperty",
        "DataProperty",
        "Annotation",
        "Individual",
        "AnnotationProperty",
        "ClassAssertion",
        "NegativeClassAssertion",
        "ObjectPropertyAssertion",
        "NegativeObjectPropertyAssertion",
        "DataPropertyAssertion",
        "NegativeDataPropertyAssertion",
        "AnnotationAssertion",
        "NegativeAnnotationAssertion",
    ];

    let web3_url = config
        .default_eth_backend_config()
        .unwrap()
        .network_address
        .as_ref()
        .unwrap();
    let library_addresses: Vec<_> = libraries
        .iter()
        .map(|library_name| {
            let (_eloop, contract) = deploy_contract(
                web3_url,
                &format!("{}Storage", library_name),
                deployer_address,
                (),
            );
            let deployed_address = contract.wait().unwrap();
            deployed_address
        })
        .collect();

    let (_eloop, rlay_token) = deploy_contract(web3_url, "RlayToken", deployer_address, ());
    let rlay_token_address = rlay_token.wait().unwrap();

    let (_eloop, ontology_storage) = deploy_contract(
        web3_url,
        "OntologyStorage",
        deployer_address,
        library_addresses,
    );
    let ontology_storage_address = ontology_storage.wait().unwrap();

    let (_eloop, proposition_ledger) = deploy_contract(
        web3_url,
        "PropositionLedger",
        deployer_address,
        (
            ethabi::Token::Address(rlay_token_address),
            ethabi::Token::Address(ontology_storage_address),
        ),
    );
    let proposition_ledger_address = proposition_ledger.wait().unwrap();

    println!("RlayToken {:?}", rlay_token_address);
    println!("OntologyStorage {:?}", ontology_storage_address);
    println!("PropositionLedger {:?}", proposition_ledger_address);
}
