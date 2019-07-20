use rustc_hex::FromHex;
use std::collections::HashMap;
use url::Url;
use web3::types::H160;
use web3::DuplexTransport;

#[derive(Debug, Deserialize, Clone)]
pub struct EthereumBackendConfig {
    #[serde(default = "default_network_address")]
    /// Address of the host networks RPC
    pub network_address: Option<String>,
    #[serde(default)]
    pub contract_addresses: HashMap<String, String>,
    // TODO: should be taken from smart contract
    #[serde(default = "default_epoch_length")]
    pub epoch_length: u64,
    #[serde(default = "default_payout_root_submission_disabled")]
    pub payout_root_submission_disabled: bool,
}

fn default_network_address() -> Option<String> {
    Some("ws://localhost:8545".to_owned())
}

fn default_epoch_length() -> u64 {
    100
}

fn default_payout_root_submission_disabled() -> bool {
    false
}

impl EthereumBackendConfig {
    pub fn contract_address(&self, name: &str) -> H160 {
        let address_bytes = self.contract_addresses.get(name).unwrap_or_else(|| {
            panic!(
                "Could not find configuration key for contract_addresses.{}",
                name
            )
        })[2..]
            .from_hex()
            .unwrap();

        H160::from_slice(&address_bytes)
    }

    pub fn web3_with_handle(
        &self,
        eloop_handle: &tokio_core::reactor::Handle,
    ) -> web3::Web3<impl DuplexTransport> {
        let network_address: Url = self.network_address.as_ref().unwrap().parse().unwrap();
        let transport = match network_address.scheme() {
            #[cfg(feature = "transport_ws")]
            "ws" => web3::transports::WebSocket::with_event_loop(
                    self
                        .network_address
                        .as_ref()
                        .unwrap(),
                    eloop_handle
                ).unwrap()
            ,
            #[cfg(feature = "transport_ipc")]
            "file" => 
                web3::transports::Ipc::with_event_loop(
                    network_address.path(),
                    eloop_handle,
                ).unwrap()
            ,
            _ => panic!(
                "Only \"file://\" (for IPC) and \"ws://\" addresses are currently supported, and the client has to be compiled with the appropriate flag (transport_ipc or transport_ws)."
            ),
        };

        web3::Web3::new(transport)
    }
}
