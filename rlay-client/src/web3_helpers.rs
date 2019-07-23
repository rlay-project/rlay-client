use rlay_backend_ethereum::data::RLAY_TOKEN_ABI;
use rustc_hex::ToHex;
use web3::Transport;

use crate::config::Config;

pub struct HexString<'a> {
    pub inner: &'a [u8],
}

impl<'a> HexString<'a> {
    pub fn fmt(bytes: &'a [u8]) -> String {
        let hex: String = bytes.to_hex();
        format!("0x{}", &hex)
    }

    pub fn wrap(bytes: &'a [u8]) -> Self {
        HexString { inner: bytes }
    }

    pub fn wrap_option(bytes: Option<&'a Vec<u8>>) -> Option<Self> {
        match bytes {
            Some(bytes) => Some(HexString { inner: bytes }),
            None => None,
        }
    }
}

impl<'a> ::serde::Serialize for HexString<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        Ok(serializer.serialize_str(&Self::fmt(self.inner))?)
    }
}

pub fn rlay_token_contract(
    config: &Config,
    web3: &web3::Web3<impl Transport>,
) -> web3::contract::Contract<impl Transport> {
    web3::contract::Contract::from_json(
        web3.eth(),
        config
            .default_eth_backend_config()
            .unwrap()
            .contract_address("RlayToken"),
        RLAY_TOKEN_ABI.as_bytes(),
    )
    .expect("Couldn't load RlayToken contract")
}
