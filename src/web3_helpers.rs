use ethabi;
use web3::Transport;
use web3::api::Eth;
use web3::types::{Address, BlockNumber, Bytes, CallRequest};
use web3::contract::Options;
use web3::contract::tokens::Tokenize;
use web3::helpers::CallFuture;

pub fn raw_query<A, B, C, P, T>(
    eth: Eth<T>,
    abi: &ethabi::Contract,
    address: C,
    func: &str,
    params: P,
    from: A,
    options: Options,
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
