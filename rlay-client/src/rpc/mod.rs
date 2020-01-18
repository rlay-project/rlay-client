mod proxy;

use ::web3::types::H160;
use cid::ToCid;
use futures::prelude::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{header, Body, Method, Request, Response, Server, StatusCode};
use rlay_backend::BackendRpcMethods;
use rlay_ontology::prelude::*;
use rlay_payout::aggregation::{detect_valued_pools, WeightedMedianBooleanPropositionPool};
use rustc_hex::{FromHex, ToHex};
use serde_json::Value;
use std::error::Error;
use std::net::ToSocketAddrs;
use tokio_futures3::runtime::Runtime;
use url::Url;

use self::proxy::proxy_rpc_call;
use crate::backend::Backend;
use crate::config::{BackendConfig, Config};
use crate::sync::MultiBackendSyncState;
use crate::web3_helpers::HexString;

const NETWORK_VERSION: &'static str = "0.3.3";
const CLIENT_VERSION: &'static str = env!("CARGO_PKG_VERSION");

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type JsonRpcResult<T> = std::result::Result<T, jsonrpc_core::Error>;

pub fn start_rpc(full_config: &Config, sync_state: MultiBackendSyncState) {
    let config = full_config.rpc.clone();
    if config.disabled {
        debug!("RPC disabled. Not starting RPC server.");
        return;
    }

    let http_proxy_config = full_config.clone();
    let http_proxy_sync_state = sync_state.clone();
    // HTTP RPC
    run_rpc_with_tokio(&http_proxy_config, http_proxy_sync_state).unwrap();
}

fn extract_option_backend(options_object: Option<Value>) -> Option<String> {
    options_object
        .as_ref()
        .and_then(|n| n.as_object())
        .and_then(|n| n.get("backend"))
        .and_then(|n| n.as_str().map(ToOwned::to_owned))
}

fn extract_options_object(params_array: &[Value], pos: usize) -> Option<Value> {
    let default_options = json!({});
    params_array
        .get(pos)
        .map(ToOwned::to_owned)
        .or_else(|| Some(default_options))
}

fn backend_from_backend_name(
    config: &Config,
    sync_state: &MultiBackendSyncState,
    backend_name: Option<String>,
) -> impl Future<Output = JsonRpcResult<Backend>> {
    config
        .get_backend_with_syncstate(backend_name.as_ref().map(|x| &**x), sync_state)
        .map_err(|_| jsonrpc_core::Error::invalid_params("Could not find specified backend"))
}

fn failure_into_jsonrpc_err(err: ::failure::Error) -> jsonrpc_core::Error {
    let mut e = jsonrpc_core::Error::internal_error();
    e.message = format!("{}", err);
    e
}

// fn generic_into_jsonrpc_err(err: GenericError) -> jsonrpc_core::Error {
// let mut e = jsonrpc_core::Error::internal_error();
// e.message = format!("{}", err);
// e
// }

async fn run_rpc(
    full_config: &Config,
    _sync_state: MultiBackendSyncState,
) -> Result<(), GenericError> {
    let addr = full_config
        .rpc
        .network_address
        .parse::<Url>()
        .expect("Unable to parse rpc.network_address")
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    let full_config = full_config.clone();
    let mut sync_state = MultiBackendSyncState::new();
    let sync_state = {
        let mut sync_state = MultiBackendSyncState::new();
        for (backend_name, config) in full_config.backends.iter() {
            sync_state
                .add_backend_conn(backend_name.clone(), config.clone())
                .await;
        }

        sync_state
    };

    let new_service = make_service_fn(move |_| {
        let full_config = full_config.clone();
        let sync_state = sync_state.clone();
        async {
            Ok::<_, GenericError>(service_fn(move |req| {
                match (req.method(), req.uri().path()) {
                    (&Method::GET, "/health") => http_get_health().boxed(),
                    _ => handle_jsonrpc(full_config.clone(), sync_state.clone(), req).boxed(),
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(new_service);

    println!("Listening on http://{}", addr);

    server.await?;

    Ok(())
}

async fn http_get_health() -> Result<Response<Body>, GenericError> {
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"status": "healthy"}"#))?;
    Ok(response)
}

pub fn run_rpc_with_tokio(
    full_config: &Config,
    sync_state: MultiBackendSyncState,
) -> Result<(), GenericError> {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(run_rpc(full_config, sync_state))
}

async fn handle_jsonrpc(
    full_config: Config,
    sync_state: MultiBackendSyncState,
    req: Request<Body>,
) -> Result<Response<Body>, GenericError> {
    let config = full_config.clone();
    let body: Vec<u8> = req.into_body().try_concat().await?.to_vec();
    let body_value: Value = serde_json::from_slice(&body).unwrap();

    let id = body_value.as_object().unwrap()["id"].clone();
    let method = body_value.as_object().unwrap()["method"].as_str().unwrap();
    let params = body_value.as_object().unwrap()["params"]
        .as_array()
        .unwrap();

    let internal_result = match method {
        "rlay_version" => Some(rpc_rlay_version(full_config).await?),
        "rlay_experimentalStoreEntity" => Some(
            rpc_rlay_experimental_store_entity(full_config, sync_state, params.to_owned()).await?,
        ),
        "rlay_experimentalStoreEntities" => Some(
            rpc_rlay_experimental_store_entities(full_config, sync_state, params.to_owned()).await?,
        ),
        "rlay_experimentalGetEntity" => Some(
            rpc_rlay_experimental_get_entity(full_config, sync_state, params.to_owned()).await?,
        ),
        "rlay_experimentalGetEntities" => Some(
            rpc_rlay_experimental_get_entities(full_config, sync_state, params.to_owned()).await?,
        ),
        "rlay_experimentalNeo4jQuery" => Some(
            rpc_rlay_experimental_neo4j_query(full_config, sync_state, params.to_owned()).await?,
        ),
        "rlay_experimentalListCids" => {
            Some(rpc_rlay_experimental_list_cids(full_config, sync_state, params.to_owned()).await?)
        }
        "rlay_experimentalGetEntityCid" => {
            Some(rpc_rlay_experimental_get_entity_cid(params.to_owned()).await?)
        }
        "rlay_getPropositionPools" => {
            Some(rpc_rlay_get_proposition_pools(full_config, sync_state, params.to_owned()).await?)
        }
        "rlay_experimentalKindForCid" => Some(
            rpc_rlay_experimental_kind_for_cid(full_config, sync_state, params.to_owned()).await?,
        ),
        "rlay_experimentalListCidsIndex" => Some(
            rpc_rlay_experimental_list_cids_index(full_config, sync_state, params.to_owned())
                .await?,
        ),
        _ => None,
    };

    let json = match internal_result {
        Some(internal_res) => {
            let json = json!({ "id": id, "jsonrpc": "2.0", "result": internal_res });
            json
        }
        None => match config.rpc.proxy_target_network_address {
            None => {
                let mut err = jsonrpc_core::Error::internal_error();
                err.message = format!("Method not found: {}", method);
                Result::Err(err)?
            }
            Some(proxy_target) => proxy_rpc_call(proxy_target, body_value).await?,
        },
    };

    let json_str = serde_json::to_string(&json)?;
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json_str))?;
    Ok(response)
}

/// `rlay_version` RPC call.
///
/// Provides version information about the network and client.
async fn rpc_rlay_version(config: Config) -> JsonRpcResult<Value> {
    let contract_addresses = match config.get_backend_config(Some("default_eth")) {
        Ok(default_eth_backend_config) => match default_eth_backend_config {
            BackendConfig::Ethereum(default_eth_backend_config) => {
                json! {{
                    "OntologyStorage": default_eth_backend_config.contract_address("OntologyStorage"),
                    "RlayToken": default_eth_backend_config.contract_address("RlayToken"),
                    "PropositionLedger": default_eth_backend_config.contract_address("PropositionLedger"),
                }}
            }
            _ => panic!("\"default_eth\" is not an Ethereum backend"),
        },
        Err(_) => {
            warn!("Running without \"default_eth\" backend. The rlay_version RPC method contains some dummy data");
            json! {{
                "OntologyStorage": H160::zero(),
                "RlayToken": H160::zero(),
                "PropositionLedger": H160::zero(),
            }}
        }
    };

    Ok(json! {{
        "networkVersion": NETWORK_VERSION,
        "clientVersion": format!("rlay-client/{}", CLIENT_VERSION),
        "contractAddresses": contract_addresses,
    }})
}

async fn rpc_rlay_experimental_store_entity(
    config: Config,
    sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let entity_object = params_array
        .get(0)
        .ok_or(jsonrpc_core::Error::invalid_params(
            "Mandatory parameter 'entity' missing",
        ))?;
    let web3_entity: FormatWeb3<Entity> = serde_json::from_value(entity_object.clone())
        .map_err(|err| jsonrpc_core::Error::invalid_params(err.description()))?;
    let entity: Entity = web3_entity.0;

    let options_object = extract_options_object(&params_array, 1);
    let backend_name = extract_option_backend(options_object.clone());

    let mut backend = backend_from_backend_name(&config, &sync_state, backend_name).await?;

    let cid = BackendRpcMethods::store_entity(&mut backend, &entity, &options_object.unwrap())
        .map_err(failure_into_jsonrpc_err)
        .map_ok(|raw_cid| {
            let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());
            serde_json::to_value(cid).unwrap()
        })
        .await
        .unwrap();

    Ok(cid)
}

async fn rpc_rlay_experimental_store_entities(
    config: Config,
    sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let entity_objects = params_array
        .get(0)
        .ok_or(jsonrpc_core::Error::invalid_params(
            "Mandatory parameter 'entities' missing",
        ))
        .unwrap()
        .as_array().unwrap();

    let entities: Vec<Entity> = entity_objects
        .iter()
        .map(|entity_object| {
            let web3_entities: FormatWeb3<Entity> = serde_json::from_value(entity_object.clone())
                .map_err(|err| jsonrpc_core::Error::invalid_params(err.description()))
                .unwrap();
            return web3_entities.0
        })
        .collect();

    let options_object = extract_options_object(&params_array, 1);
    let backend_name = extract_option_backend(options_object.clone());

    let mut backend = backend_from_backend_name(&config, &sync_state, backend_name).await?;

    let cids = BackendRpcMethods::store_entities(&mut backend, &entities, &options_object.unwrap())
        .map_err(failure_into_jsonrpc_err)
        .map_ok(|raw_cids| {
            return raw_cids.iter().map(|raw_cid| {
                let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());
                serde_json::to_value(cid).unwrap()
            })
            .collect();
        })
        .await?;

    Ok(cids)
}

async fn rpc_rlay_experimental_get_entity(
    config: Config,
    sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let cid = params_array.get(0).unwrap().as_str().unwrap().to_owned();

    let options_object = extract_options_object(&params_array, 1);
    let backend_name = extract_option_backend(options_object);

    let mut backend = backend_from_backend_name(&config, &sync_state, backend_name).await?;

    let entity: serde_json::Value = BackendRpcMethods::get_entity(&mut backend, &cid)
        .map_err(failure_into_jsonrpc_err)
        .map_ok(move |entity| {
            debug!("retrieved {:?}", entity.is_some());
            serde_json::to_value(entity.map(|n| FormatWeb3(n))).unwrap()
        })
        .await
        .unwrap();

    Ok(entity)
}

async fn rpc_rlay_experimental_get_entities(
    config: Config,
    sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let cid_array = params_array.get(0).unwrap().as_array().unwrap().to_owned();

    let cids: Vec<String> = cid_array
        .iter()
        .map(|cid_value| {
            return cid_value.as_str().unwrap().to_owned();
        })
        .collect();

    let options_object = extract_options_object(&params_array, 1);
    let backend_name = extract_option_backend(options_object);

    let mut backend = backend_from_backend_name(&config, &sync_state, backend_name).await?;

    let result: serde_json::Value = BackendRpcMethods::get_entities(&mut backend, cids)
        .map_err(failure_into_jsonrpc_err)
        .map_ok(|raw_entities| {
            return raw_entities.iter().map(|raw_entity| {
                serde_json::to_value(FormatWeb3(raw_entity)).unwrap()
            })
            .collect();
        })
        .await
        .unwrap();

    Ok(result)
}

async fn rpc_rlay_experimental_neo4j_query(
    config: Config,
    sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let filter_registry = crate::modules::ModuleRegistry::with_builtins();

    let query = params_array.get(0).unwrap().as_str().unwrap().to_owned();

    let default_options = json!({});
    let options_object = params_array.get(1).or_else(|| Some(&default_options));
    let backend_name = extract_option_backend(options_object.map(|n| n.clone()).clone());

    let activated_filters_names: Vec<String> = options_object
        .and_then(|n| n.as_object())
        .and_then(|n| n.get("filters"))
        .and_then(|n| {
            n.as_array().and_then(|filters_arr| {
                Some(
                    filters_arr
                        .into_iter()
                        .map(|n| n.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                )
            })
        })
        .unwrap_or_else(Vec::new);

    let config = config.clone();
    let sync_state = sync_state.clone();
    let filter_registry = filter_registry.clone();

    let mut backend = backend_from_backend_name(&config, &sync_state, backend_name).await?;

    let cids: Vec<String> = BackendRpcMethods::neo4j_query(&mut backend, &query)
        .map_err(failure_into_jsonrpc_err)
        .await
        .unwrap();

    let activated_filters: Vec<_> = activated_filters_names
        .into_iter()
        .filter_map(|filter_name| filter_registry.filter(&filter_name.to_owned()))
        .collect();

    let entities = backend
        .get_entities(cids)
        .map_err(failure_into_jsonrpc_err)
        .await
        .unwrap();

    let filtered_entities = entities
        .into_iter()
        .filter(|entity| {
            for filter in &activated_filters {
                if !filter.lock().unwrap().filter(entity.clone()) {
                    return false;
                }
            }
            return true;
        })
        .map(|entity| FormatWeb3(entity))
        .collect::<Vec<_>>();

    Ok(serde_json::to_value(filtered_entities).unwrap())
}

/// `rlay_experimentalListCids` RPC call.
///
/// List all CIDs seen via "<Entity>Stored" events.
async fn rpc_rlay_experimental_list_cids(
    config: Config,
    sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let entity_kind: Option<String> = params_array.get(0).unwrap().as_str().map(|n| n.to_owned());

    let options_object = extract_options_object(&params_array, 1);
    let backend_name = extract_option_backend(options_object);

    let mut backend = backend_from_backend_name(&config, &sync_state, backend_name).await?;

    let cids: Vec<String> =
        BackendRpcMethods::list_cids(&mut backend, entity_kind.as_ref().map(|n| &**n))
            .map_err(failure_into_jsonrpc_err)
            .await
            .unwrap();

    Ok(serde_json::to_value(cids).unwrap())
}

async fn rpc_rlay_experimental_get_entity_cid(params_array: Vec<Value>) -> JsonRpcResult<Value> {
    let entity_object = params_array.get(0).unwrap();
    let web3_entity: FormatWeb3<Entity> = serde_json::from_value(entity_object.clone())
        .map_err(|err| jsonrpc_core::Error::invalid_params(err.description()))
        .unwrap();
    let entity: Entity = web3_entity.0;
    let cid: String = format!("0x{}", entity.to_cid().unwrap().to_bytes().to_hex());

    Ok(serde_json::to_value(cid).unwrap())
}

/// `rlay_getPropositionPools` RPC call.
///
/// Lists proposition pools.
async fn rpc_rlay_get_proposition_pools(
    _config: Config,
    multi_sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let sync_state = multi_sync_state
        .backend("default_eth")
        .expect("Running without backend default_eth")
        .as_ethereum()
        .unwrap();

    let proposition_ledger = sync_state.proposition_ledger.lock().unwrap();
    let raw_entity_map = sync_state.entity_map();
    let entity_map = raw_entity_map.lock().unwrap();

    let relevant_propositions: Vec<_> = proposition_ledger.iter().collect();

    let entities: Vec<_> = entity_map.values().collect();
    let mut pools = detect_valued_pools(&entities, &relevant_propositions);

    if let Value::Object(ref params_map) = params_array[0] {
        if let Some(param_subject) = params_map.get("subject") {
            let value = |n: &WeightedMedianBooleanPropositionPool| {
                serde_json::to_value(HexString::fmt(n.pool.subject())).unwrap()
            };
            pools.retain(|n| &value(n) == param_subject);
        }
        if let Some(param_subject_property) = params_map.get("subjectProperty") {
            let value = |n: &WeightedMedianBooleanPropositionPool| {
                let vals: Vec<_> = n
                    .pool
                    .subject_property()
                    .into_iter()
                    .map(HexString::fmt)
                    .collect();
                serde_json::to_value(vals).unwrap()
            };
            pools.retain(|n| &value(n) == param_subject_property);
        }
        if let Some(param_target) = params_map.get("target") {
            let value = |n: &WeightedMedianBooleanPropositionPool| {
                if n.pool.target().is_none() {
                    return serde_json::to_value(()).unwrap();
                }
                serde_json::to_value(HexString::fmt(n.pool.target().unwrap())).unwrap()
            };
            pools.retain(|n| &value(n) == param_target);
        }
    }

    Ok(serde_json::to_value(pools).unwrap())
}

/// `rlay_experimentalKindForCid` RPC call.
///
/// Allows to resolve the kind for all the CIDs the client has seen via "<Entity>Stored" events.
async fn rpc_rlay_experimental_kind_for_cid(
    _config: Config,
    multi_sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let sync_state = multi_sync_state
        .backend("default_eth")
        .expect("Running without backend default_eth")
        .as_ethereum()
        .unwrap();

    let cid_entity_kind_map = sync_state.cid_entity_kind_map();
    let cid_entity_kind_map_lock = cid_entity_kind_map.lock().unwrap();

    let cid = params_array.get(0).unwrap().as_str().unwrap();
    let cid_no_prefix = str::replace(cid, "0x", "");
    let cid_bytes = cid_no_prefix.from_hex().unwrap();

    let entity_kind = cid_entity_kind_map_lock.get(&cid_bytes);
    Ok(json! {{
        "cid": cid,
        "kind": entity_kind,
    }})
}

async fn rpc_rlay_experimental_list_cids_index(
    _config: Config,
    multi_sync_state: MultiBackendSyncState,
    params_array: Vec<Value>,
) -> JsonRpcResult<Value> {
    let sync_state = multi_sync_state
        .backend("default_eth")
        .expect("Running without backend default_eth")
        .as_ethereum()
        .unwrap();

    let entity_map = sync_state.entity_map();
    let entity_map_lock = entity_map.lock().unwrap();

    let cids: Vec<_> = match (
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
                        let entity_json =
                            serde_json::to_value(FormatWeb3((*entity).clone())).unwrap();
                        let field_val = &entity_json[field];
                        match field_val {
                            Value::Array(json_values) => {
                                let values: Vec<_> =
                                    json_values.iter().map(|n| n.as_str().unwrap()).collect();
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
    };

    Ok(serde_json::to_value(cids).unwrap())
}
