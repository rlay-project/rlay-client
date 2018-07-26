mod proxy;

use jsonrpc_core::*;
use jsonrpc_http_server::*;

use self::proxy::ProxyHandler;
use config::RpcConfig;

const NETWORK_VERSION: &'static str = "0.2.0";
const CLIENT_VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn start_rpc(config: &RpcConfig) {
    if config.disabled {
        debug!("RPC disabled. Not starting RPC server.");
        return;
    }

    let mut io = ProxyHandler::new(config.proxy_target_network_address.as_ref().unwrap());
    io.add_method("rlay_version", rpc_rlay_version());

    let _server = ServerBuilder::new(io)
        .start_http(&config.network_address.parse().unwrap())
        .expect("Unable to start RPC server");

    _server.wait();
}

/// `rlay_version` RPC call.
///
/// Provides version information about the network and client.
fn rpc_rlay_version() -> impl RpcMethodSimple {
    |_: Params| {
        Ok(json!{{
            "networkVersion": NETWORK_VERSION,
            "clientVersion": format!("rlay-client/{}", CLIENT_VERSION),
        }})
    }
}
