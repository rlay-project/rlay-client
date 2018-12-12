use ethabi;
use multibase::{encode as base_encode, Base};
use rustc_hex::ToHex;
use web3::api::Eth;
use web3::contract::tokens::Tokenize;
use web3::contract::Options;
use web3::helpers::CallFuture;
use web3::types::{Address, BlockNumber, Bytes, CallRequest};
use web3::Transport;

pub fn raw_query<A, B, C, P, T>(
    eth: &Eth<T>,
    abi: &ethabi::Contract,
    address: C,
    func: &str,
    params: P,
    from: A,
    options: &Options,
    block: B,
) -> CallFuture<Bytes, T::Out>
where
    A: Into<Option<Address>>,
    B: Into<Option<BlockNumber>>,
    C: Into<Address>,
    P: Tokenize,
    T: Transport,
{
    abi.function(func.into())
        .and_then(|function| function.encode_input(&params.into_tokens()))
        .map(|call| {
            eth.call(
                CallRequest {
                    from: from.into(),
                    to: address.into(),
                    gas: options.gas,
                    gas_price: options.gas_price,
                    value: options.value,
                    data: Some(Bytes(call)),
                },
                block.into(),
            )
        })
        .unwrap()
    // .unwrap_or_else(Into::into)
}

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

pub fn base58_encode(data: &[u8]) -> String {
    base_encode(Base::Base58btc, data)
}
