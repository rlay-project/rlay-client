mod proxy;

use jsonrpc_core::*;
use jsonrpc_http_server::*;
use serde_json;
use rlay_ontology::ontology::Annotation;
use ethabi::token::{StrictTokenizer, Token, Tokenizer};
use ethabi;
use rustc_hex::{FromHex, ToHex};

use config::Config;
use payout_calculation::detect_pools;
use self::proxy::ProxyHandler;
use sync::SyncState;
use sync_ontology::{entity_map_class_assertions, entity_map_individuals,
                    entity_map_negative_class_assertions};

const NETWORK_VERSION: &'static str = "0.2.0";
const CLIENT_VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn start_rpc(full_config: &Config, sync_state: SyncState) {
    let config = full_config.rpc.clone();
    if config.disabled {
        debug!("RPC disabled. Not starting RPC server.");
        return;
    }

    let mut io = ProxyHandler::new(config.proxy_target_network_address.as_ref().unwrap());
    io.add_method("rlay_version", rpc_rlay_version(full_config));
    io.add_method(
        "rlay_getPropositionPools",
        rpc_rlay_get_proposition_pools(sync_state.clone()),
    );
    io.add_method("rlay_encodeForStore", rpc_rlay_encode_for_store());
    io.add_method(
        "rlay_experimentalKindForCid",
        rpc_rlay_experimental_kind_for_cid(sync_state.clone()),
    );
    io.add_method(
        "rlay_experimentalListCids",
        rpc_rlay_experimental_list_cids(sync_state.clone()),
    );

    let _server = ServerBuilder::new(io)
        .start_http(&config.network_address.parse().unwrap())
        .expect("Unable to start RPC server");

    _server.wait();
}

/// `rlay_version` RPC call.
///
/// Provides version information about the network and client.
fn rpc_rlay_version(config: &Config) -> impl RpcMethodSimple {
    let config = config.clone();
    move |_: Params| {
        Ok(json!{{
            "networkVersion": NETWORK_VERSION,
            "clientVersion": format!("rlay-client/{}", CLIENT_VERSION),
            "contractAddresses": {
                "OntologyStorage": config.contract_address("OntologyStorage"),
                "RlayToken": config.contract_address("RlayToken"),
                "PropositionLedger": config.contract_address("PropositionLedger"),
            }
        }})
    }
}

/// `rlay_getPropositionPools` RPC call.
///
/// Lists proposition pools.
fn rpc_rlay_get_proposition_pools(sync_state: SyncState) -> impl RpcMethodSimple {
    move |_: Params| {
        let proposition_ledger = sync_state.proposition_ledger.lock().unwrap();
        let entity_map = sync_state.entity_map.lock().unwrap();

        let relevant_propositions: Vec<_> = proposition_ledger.iter().collect();
        let ontology_individuals = entity_map_individuals(&entity_map);
        let ontology_class_assertions = entity_map_class_assertions(&entity_map);
        let ontology_negative_class_assertions = entity_map_negative_class_assertions(&entity_map);

        let pools = detect_pools(
            &ontology_individuals,
            &ontology_class_assertions,
            &ontology_negative_class_assertions,
            &relevant_propositions,
            false,
        );
        Ok(serde_json::to_value(pools).unwrap())
    }
}

fn annotation_from_params(params: &Params) -> ::std::result::Result<Annotation, String> {
    println!("PARAM1 {:?}", params);
    if let Params::Array(params_array) = params {
        println!("PARAM {:?}", params_array[0]);
        if let Value::Object(ref params_map) = params_array[0] {
            let mut annotation = Annotation::default();

            let param_annotations = params_map.get("annotations");
            let param_property = params_map.get("property");
            let param_value = params_map.get("value");

            if let Some(param_annotations) = param_annotations {
                let param_annotations = param_annotations
                    .as_array()
                    .ok_or("Param annotations is not an array".to_owned())?;
                let annotations: Vec<_> = param_annotations
                    .iter()
                    .map(|n| StrictTokenizer::tokenize_bytes(n.as_str().unwrap()).unwrap())
                    .collect();
                annotation.annotations = annotations;
            }

            Ok(annotation)
        } else {
            Err("First params has to be a object".to_owned())
        }
    } else {
        Err("Params has to be an array with single object".to_owned())
    }
}

fn annotation_to_tokens(annotation: Annotation) -> Vec<Token> {
    vec![
        Token::Array(
            annotation
                .annotations
                .into_iter()
                .map(|n| Token::Bytes(n))
                .collect(),
        ),
        Token::Bytes(annotation.property),
        Token::Bytes(annotation.value),
    ]
}

/// `rlay_encodeForStore` RPC call.
///
/// Encodes the `data` for a `store<Entity>` contract call.
fn rpc_rlay_encode_for_store() -> impl RpcMethodSimple {
    move |params: Params| {
        let ontology_contract_abi = include_str!("../../data/OntologyStorage.abi");
        let contract = ethabi::Contract::load(ontology_contract_abi.as_bytes()).unwrap();

        let annotation = annotation_from_params(&params).expect("Could not parse annotation");
        let tokens = annotation_to_tokens(annotation);
        let function = contract
            .function("storeAnnotation")
            .expect("Could not find function");

        let data = function.encode_input(&tokens).unwrap().to_hex();

        Ok(json!{{
            "data": data,
        }})
    }
}

/// `rlay_experimentalKindForCid` RPC call.
///
/// Allows to resolve the kind for all the CIDs the client has seen via "<Entity>Stored" events.
fn rpc_rlay_experimental_kind_for_cid(sync_state: SyncState) -> impl RpcMethodSimple {
    move |params: Params| {
        if let Params::Array(params_array) = params {
            let cid_entity_kind_map = sync_state.cid_entity_kind_map();
            let cid_entity_kind_map_lock = cid_entity_kind_map.lock().unwrap();

            let cid = params_array.get(0).unwrap().as_str().unwrap();
            let cid_no_prefix = str::replace(cid, "0x", "");
            let cid_bytes = cid_no_prefix.from_hex().unwrap();

            let entity_kind = cid_entity_kind_map_lock.get(&cid_bytes);
            Ok(json!{{
                "cid": cid,
                "kind": entity_kind,
            }})
        } else {
            unimplemented!()
        }
    }
}

/// `rlay_experimentalListCids` RPC call.
///
/// List all CIDs seen via "<Entity>Stored" events.
fn rpc_rlay_experimental_list_cids(sync_state: SyncState) -> impl RpcMethodSimple {
    move |_: Params| {
        let cid_entity_kind_map = sync_state.cid_entity_kind_map();
        let cid_entity_kind_map_lock = cid_entity_kind_map.lock().unwrap();

        let cids: Vec<_> = cid_entity_kind_map_lock
            .keys()
            .map(|n| format!("0x{}", n.to_hex()))
            .collect();

        Ok(serde_json::to_value(cids).unwrap())
    }
}
