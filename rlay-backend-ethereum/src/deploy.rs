use futures::compat::Future01CompatExt;
use futures::prelude::*;
use rustc_hex::FromHex;
use serde_derive::Deserialize;
use web3::types::Address;
use web3::Transport;

use crate::data::contract_bins;

#[derive(Deserialize)]
struct ContractData {
    pub abi: serde_json::Value,
    pub bytecode: web3::types::Bytes,
}

pub fn deploy_contract<T: web3::contract::tokens::Tokenize + Clone>(
    web3: &web3::Web3<impl Transport>,
    contract_name: &str,
    deployer_address: &str,
    constructor_params: T,
) -> impl Future<Output = Result<(String, Address), ()>> {
    let contract_name = contract_name.to_owned();
    let bins = contract_bins();
    let contract_data: ContractData =
        serde_json::from_str(bins.get(AsRef::<str>::as_ref(&contract_name)).unwrap())
            .expect("Can't read contract data");

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

    deployed_contract
        .compat()
        .map_ok(move |contract| (contract_name.to_owned(), contract.address()))
        .map_err(|_| ())
}

pub fn deploy_library_contracts<'a>(
    web3: &'a web3::Web3<impl Transport>,
    deployer_address: &'a str,
) -> impl Stream<Item = Result<(String, Address), ()>> + 'a {
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

    stream::iter(libraries).then(move |library_name| {
        let contract_name = format!("{}Storage", library_name);
        deploy_contract(web3, &contract_name, deployer_address, ())
    })
}

pub fn deploy_contracts<'a>(
    web3: &'a web3::Web3<impl Transport>,
    deployer_address: &'a str,
) -> impl Stream<Item = (String, Address)> + 'a {
    let libraries_deployed = deploy_library_contracts(&web3, deployer_address.clone())
        .try_collect::<Vec<_>>()
        .shared();

    let library_addresses_fut = libraries_deployed
        .clone()
        .map_ok(|deployed_contract| {
            deployed_contract
                .into_iter()
                .map(|n| n.1)
                .collect::<Vec<_>>()
        })
        .shared();

    let deployer_address1 = deployer_address.clone();
    let rlay_token_deployed = library_addresses_fut
        .clone()
        .and_then(move |_| deploy_contract(&web3, "RlayToken", deployer_address1, ()))
        .shared();
    let rlay_token_address_fut = rlay_token_deployed
        .clone()
        .map_ok(|deployed_contract| deployed_contract.1);

    let deployer_address2 = deployer_address.clone();
    let ontology_storage_deployed = library_addresses_fut
        .clone()
        .and_then(move |library_addresses| {
            deploy_contract(
                &web3,
                "OntologyStorage",
                deployer_address2,
                library_addresses,
            )
        })
        .shared();
    let ontology_storage_address_fut = ontology_storage_deployed
        .clone()
        .map_ok(|deployed_contract| deployed_contract.1);

    let deployer_address3 = deployer_address.clone();
    let proposition_ledger_deployed =
        future::try_join(rlay_token_address_fut, ontology_storage_address_fut)
            .and_then(move |(rlay_token_address, ontology_storage_address)| {
                deploy_contract(
                    &web3,
                    "PropositionLedger",
                    deployer_address3,
                    (
                        ethabi::Token::Address(rlay_token_address),
                        ethabi::Token::Address(ontology_storage_address),
                    ),
                )
            })
            .shared();

    let libraries_stream = libraries_deployed
        .map_ok(|library_addresses| stream::iter(library_addresses))
        .unwrap_or_else(|_| stream::iter(vec![]))
        .flatten_stream();
    let rlay_token_stream = rlay_token_deployed
        .clone()
        .unwrap_or_else(|_| ("Unknown".to_string(), Address::zero()))
        .into_stream();
    let ontology_storage_stream = ontology_storage_deployed
        .clone()
        .unwrap_or_else(|_| ("Unknown".to_string(), Address::zero()))
        .into_stream();
    let proposition_ledger_stream = proposition_ledger_deployed
        .clone()
        .unwrap_or_else(|_| ("Unknown".to_string(), Address::zero()))
        .into_stream();

    libraries_stream
        .chain(rlay_token_stream)
        .chain(ontology_storage_stream)
        .chain(proposition_ledger_stream)
}
