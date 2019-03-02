use rustc_hex::FromHex;
use serde_derive::Deserialize;
use std::fs::File;
use std::process::Command;
use web3::futures::future::Future;
use web3::types::Address;

#[derive(Deserialize)]
struct ContractData {
    pub abi: serde_json::Value,
    pub bytecode: web3::types::Bytes,
}

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

fn deploy_contract<T: web3::contract::tokens::Tokenize + Clone>(
    contract_name: &str,
    deployer_address: &str,
    constructor_params: T,
) -> (
    web3::transports::EventLoopHandle,
    impl Future<Item = Address, Error = ()>,
) {
    let file = File::open(format!("./tests/test_contracts/{}.json", contract_name)).expect(
        &format!("Unable to read contract data file for {}", contract_name),
    );
    let contract_data: ContractData =
        serde_json::from_reader(file).expect("Can't read contract data");

    let (_eloop, transport) = web3::transports::WebSocket::new("ws://localhost:9545").unwrap();
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

    let deployer_address = "0xc02345a911471fd46c47c4d3c2e5c85f5ae93d13";
    let library_addresses: Vec<_> = libraries
        .iter()
        .map(|library_name| {
            let (_eloop, contract) =
                deploy_contract(&format!("{}Storage", library_name), deployer_address, ());
            let deployed_address = contract.wait().unwrap();
            deployed_address
        })
        .collect();

    let (_eloop, rlay_token) = deploy_contract("RlayToken", deployer_address, ());
    let rlay_token_address = rlay_token.wait().unwrap();

    let (_eloop, ontology_storage) =
        deploy_contract("OntologyStorage", deployer_address, library_addresses);
    let ontology_storage_address = ontology_storage.wait().unwrap();

    let (_eloop, proposition_ledger) = deploy_contract(
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
