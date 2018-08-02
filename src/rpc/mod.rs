mod proxy;

use jsonrpc_core::*;
use jsonrpc_http_server::*;
use serde_json;

use config::Config;
use payout_calculation::detect_pools;
use self::proxy::ProxyHandler;
use sync::SyncState;
use sync_ontology::entity_map_individuals;

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
        rpc_rlay_get_proposition_pools(sync_state),
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
        let pools = detect_pools(&ontology_individuals, &relevant_propositions, false);
        Ok(serde_json::to_value(pools).unwrap())
    }
}
