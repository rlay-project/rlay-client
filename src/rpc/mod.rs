mod proxy;

use std::error::Error;
use cid::ToCid;
use jsonrpc_core::{self, *};
use jsonrpc_http_server::ServerBuilder as HttpServerBuilder;
use jsonrpc_ws_server::{RequestContext, ServerBuilder as WsServerBuilder};
use jsonrpc_pubsub::{PubSubHandler, Session, Subscriber, SubscriptionId};
use serde_json;
use ethabi::token::{StrictTokenizer, Token, Tokenizer};
use ethabi;
use rlay_ontology::ontology::{Annotation, Entity};
use rustc_hex::{FromHex, ToHex};
use url::Url;
use jsonrpc_core::futures::Future;
use std::{thread, time};
use std::sync::Arc;
use web3::futures::prelude::*;

use config::{self, Config};
use aggregation::{detect_valued_pools, ValuedBooleanPropositionPool};
use self::proxy::ProxyHandler;
use sync::SyncState;
use web3_helpers::HexString;

const NETWORK_VERSION: &'static str = "0.3.0";
const CLIENT_VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn start_rpc(full_config: &Config, sync_state: SyncState) {
    let config = full_config.rpc.clone();
    if config.disabled {
        debug!("RPC disabled. Not starting RPC server.");
        return;
    }

    let http_proxy_config = full_config.clone();
    let http_proxy_sync_state = sync_state.clone();
    // HTTP RPC
    thread::spawn(move || {
        let io = proxy_handler_with_methods(&http_proxy_config, http_proxy_sync_state);

        let address: Url = http_proxy_config.rpc.network_address.parse().unwrap();
        let server = HttpServerBuilder::new(io)
            .start_http(&format!(
                "{}:{}",
                address.host_str().unwrap(),
                address.port().unwrap()
            ).parse()
                .unwrap())
            .expect("Unable to start RPC server");
        server.wait();
    });

    let sub_sync_state = sync_state.clone();
    let io = proxy_handler_with_methods(&full_config, sync_state);
    let mut handler: PubSubHandler<proxy::WebsocketMetadata, proxy::ProxyMiddleware> =
        From::from(io);
    handler.add_subscription(
        "rlay_subscribeEntities",
        (
            "rlay_subscribeEntities",
            move |params: Params, meta: proxy::WebsocketMetadata, subscriber: Subscriber| {
                let mut param_from_block: Option<u64> = None;
                if let Params::Array(params_array) = params {
                    if let Value::Object(ref params_map) = params_array[0] {
                        if let Some(raw_param_from_block) = params_map.get("fromBlock") {
                            param_from_block = Some(raw_param_from_block.as_u64().unwrap());
                        }
                    }
                }

                // TODO: use correct ids - currently ony one subscription per sesssion (= websocket
                // connection)
                let sink = subscriber
                    .assign_id(SubscriptionId::Number(meta.session_id))
                    .unwrap();
                let entity_map = sub_sync_state.entity_map();
                let mut entity_map_lock = entity_map.lock().unwrap();
                let block_entity_map = sub_sync_state.block_entity_map();
                let block_entity_map_lock = block_entity_map.lock().unwrap();
                let entity_stream = entity_map_lock
                    .on_insert_entity_with_replay(param_from_block, &block_entity_map_lock);
                let mut mapped_stream = entity_stream
                    .and_then(|entity| {
                        Ok(Params::Array(vec![serde_json::to_value(entity).unwrap()]))
                    })
                    .map_err(|_| panic!());

                // TODO: handling this with sleep still doesn't seem like the right way
                thread::spawn(move || loop {
                    match mapped_stream.poll() {
                        Ok(Async::Ready(value)) => {
                            sink.notify(value.unwrap()).wait().unwrap();
                        }
                        Ok(Async::NotReady) => thread::sleep(time::Duration::from_millis(100)),
                        _ => {}
                    }
                });
            },
        ),
        ("rlay_unsubscribeEntities", |_id: SubscriptionId| {
            println!("Closing subscription");
            futures::future::ok(Value::Bool(true))
        }),
    );

    let address: Url = config.ws_network_address.unwrap().parse().unwrap();
    let server = WsServerBuilder::new(handler)
        .session_meta_extractor(|context: &RequestContext| {
            proxy::WebsocketMetadata::new(
                Some(Arc::new(Session::new(context.sender()))),
                context.session_id,
                context.remote.clone(),
            )
        })
        .start(&format!(
            "{}:{}",
            address.host_str().unwrap(),
            address.port().unwrap()
        ).parse()
            .unwrap())
        .expect("Unable to start RPC server");
    server.wait().unwrap();
}

pub fn proxy_handler_with_methods(
    full_config: &Config,
    sync_state: SyncState,
) -> ProxyHandler<proxy::NoopPubSubMetadata> {
    let mut io = ProxyHandler::new_with_noop(
        full_config
            .rpc
            .proxy_target_network_address
            .as_ref()
            .unwrap(),
    );
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
    io.add_method(
        "rlay_experimentalListCidsIndex",
        rpc_rlay_experimental_list_cids_index(sync_state.clone()),
    );
    io.add_method(
        "rlay_experimentalGetEntity",
        rpc_rlay_experimental_get_entity(sync_state.clone()),
    );
    io.add_method(
        "rlay_experimentalGetEntityCid",
        rpc_rlay_experimental_get_entity_cid(),
    );
    io.add_method(
        "rlay_experimentalStoreEntity",
        rpc_rlay_experimental_store_entity(full_config),
    );

    io
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
    move |params: Params| {
        let proposition_ledger = sync_state.proposition_ledger.lock().unwrap();
        let raw_entity_map = sync_state.entity_map();
        let entity_map = raw_entity_map.lock().unwrap();

        let relevant_propositions: Vec<_> = proposition_ledger.iter().collect();

        let entities: Vec<_> = entity_map.values().collect();
        let mut pools = detect_valued_pools(&entities, &relevant_propositions);

        if let Params::Array(params_array) = params {
            if let Value::Object(ref params_map) = params_array[0] {
                if let Some(param_subject) = params_map.get("subject") {
                    let value = |n: &ValuedBooleanPropositionPool| {
                        serde_json::to_value(HexString::fmt(n.pool.subject())).unwrap()
                    };
                    pools.retain(|n| &value(n) == param_subject);
                }
                if let Some(param_subject_property) = params_map.get("subjectProperty") {
                    let value = |n: &ValuedBooleanPropositionPool| {
                        let vals: Vec<_> = n.pool
                            .subject_property()
                            .into_iter()
                            .map(HexString::fmt)
                            .collect();
                        serde_json::to_value(vals).unwrap()
                    };
                    pools.retain(|n| &value(n) == param_subject_property);
                }
                if let Some(param_target) = params_map.get("target") {
                    let value = |n: &ValuedBooleanPropositionPool| {
                        if n.pool.target().is_none() {
                            return serde_json::to_value(()).unwrap();
                        }
                        serde_json::to_value(HexString::fmt(n.pool.target().unwrap())).unwrap()
                    };
                    pools.retain(|n| &value(n) == param_target);
                }
            }
        }

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
            // let param_property = params_map.get("property");
            // let param_value = params_map.get("value");

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
    move |params: Params| {
        let cid_entity_kind_map = sync_state.cid_entity_kind_map();
        let cid_entity_kind_map_lock = cid_entity_kind_map.lock().unwrap();

        let cids: Vec<_> = match params {
            Params::Array(params_array) => match params_array.get(0) {
                Some(first_param) => match first_param.as_str() {
                    Some(entity_kind) => cid_entity_kind_map_lock
                        .iter()
                        .filter(|(&_, ref value)| value == &entity_kind)
                        .map(|(key, _)| format!("0x{}", key.to_hex()))
                        .collect(),
                    None => cid_entity_kind_map_lock
                        .keys()
                        .map(|n| format!("0x{}", n.to_hex()))
                        .collect(),
                },
                None => cid_entity_kind_map_lock
                    .keys()
                    .map(|n| format!("0x{}", n.to_hex()))
                    .collect(),
            },
            _ => cid_entity_kind_map_lock
                .keys()
                .map(|n| format!("0x{}", n.to_hex()))
                .collect(),
        };

        Ok(serde_json::to_value(cids).unwrap())
    }
}

fn rpc_rlay_experimental_list_cids_index(sync_state: SyncState) -> impl RpcMethodSimple {
    move |params: Params| {
        let entity_map = sync_state.entity_map();
        let entity_map_lock = entity_map.lock().unwrap();

        let cids: Vec<_> = match params {
            Params::Array(params_array) => {
                match (
                    params_array.get(0),
                    params_array.get(1),
                    params_array.get(2),
                ) {
                    (Some(kind), Some(field), Some(value)) => {
                        match (kind.as_str(), field.as_str(), value.as_str()) {
                            (Some(kind), Some(field), Some(value)) => entity_map_lock
                                .iter()
                                .filter(|(_, entity)| &Into::<&str>::into(entity.kind()) == &kind)
                                .filter(|(_, entity)| {
                                    let entity_json = serde_json::to_value(entity).unwrap();
                                    let field_val = &entity_json[field];
                                    match field_val {
                                        Value::Array(json_values) => {
                                            let values: Vec<_> = json_values.iter().map(|n| n.as_str().unwrap()).collect();
                                            return values.contains(&value);
                                        }
                                        Value::String(string_value) => return string_value == value,
                                        _ => false,
                                    }
                                })
                                .map(|(key, _)| format!("0x{}", key.to_hex()))
                                .collect(),
                            _ => Vec::new(),
                        }
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };

        Ok(serde_json::to_value(cids).unwrap())
    }
}

fn rpc_rlay_experimental_get_entity(sync_state: SyncState) -> impl RpcMethodSimple {
    move |params: Params| {
        if let Params::Array(params_array) = params {
            let entity_map = sync_state.entity_map();
            let entity_map_lock = entity_map.lock().unwrap();

            let cid = params_array.get(0).unwrap().as_str().unwrap();
            let cid_no_prefix = str::replace(cid, "0x", "");
            let cid_bytes = cid_no_prefix.from_hex().unwrap();

            let entity_kind = entity_map_lock.get(&cid_bytes);
            debug!("retrieved {:?}", entity_kind.is_some());
            Ok(serde_json::to_value(entity_kind).unwrap())
        } else {
            unimplemented!()
        }
    }
}

fn rpc_rlay_experimental_get_entity_cid() -> impl RpcMethodSimple {
    move |params: Params| {
        if let Params::Array(params_array) = params {
            let entity_object = params_array.get(0).unwrap();
            let entity: Entity = serde_json::from_value(entity_object.clone())
                .map_err(|err| jsonrpc_core::Error::invalid_params(err.description()))?;
            let cid: String = format!("0x{}", entity.to_cid().unwrap().to_bytes().to_hex());

            Ok(serde_json::to_value(cid).unwrap())
        } else {
            unimplemented!()
        }
    }
}

fn rpc_rlay_experimental_store_entity(config: &Config) -> impl RpcMethodSimple {
    let config = config.clone();
    move |params: Params| {
        if let Params::Array(params_array) = params {
            let entity_object = params_array.get(0).unwrap();
            let entity: Entity = serde_json::from_value(entity_object.clone())
                .map_err(|err| jsonrpc_core::Error::invalid_params(err.description()))?;
            let cid: String = format!("0x{}", entity.to_cid().unwrap().to_bytes().to_hex());

            let options_object = params_array.get(1).unwrap();
            let backend_name = options_object
                .as_object()
                .unwrap()
                .get("backend")
                .unwrap()
                .as_str()
                .unwrap();
            let backend_config: &config::BackendConfig = config.backends.get(backend_name).unwrap();
            match backend_config {
                #[cfg(feature = "backend_neo4j")]
                config::BackendConfig::Neo4j(backend) => {
                    let client = backend.client().unwrap();
                    let kind_name: &str = entity.kind().into();
                    let entity_val = serde_json::to_value(entity).unwrap();
                    let val = entity_val.as_object().unwrap();
                    let mut values = Vec::new();
                    let mut relationships = Vec::new();
                    // TODO: support non-array relationships
                    for (key, value) in val {
                        if let Value::Array(array_val) = value {
                            for relationship_value in array_val {
                                let rel_query = format!(
                                    "MATCH (n {{ cid: \"{0}\"}}) MERGE (m {{ cid: {2} }}) MERGE (n)-[r:{1}]->(m)",
                                    cid, key, relationship_value
                                );
                                relationships.push(rel_query);
                            }
                            continue;
                        }
                        values.push(format!("n.{0} = {1}", key, value));
                    }

                    let mut query = format!("MERGE (n {{cid: \"{1}\"}}) SET n:{0}", kind_name, cid);
                    if !values.is_empty() {
                        query.push_str(", ");
                        query.push_str(&values.join(", "));
                    }

                    trace!("NEO4J QUERY: {}", query);
                    client.exec(query).unwrap();
                    for relationship in relationships {
                        trace!("NEO4J QUERY: {}", relationship);
                        client.exec(relationship).unwrap();
                    }
                }
                #[cfg(not(feature = "backend_neo4j"))]
                config::BackendConfig::Neo4j(_) => {}
            }

            Ok(serde_json::to_value(cid).unwrap())
        } else {
            unimplemented!()
        }
    }
}
