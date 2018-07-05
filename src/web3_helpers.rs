use ethabi;
use web3::Transport;
use web3::api::Eth;
use web3::types::{Address, BlockNumber, Bytes, CallRequest, U256};
use web3::contract::Options;
use web3::contract::tokens::Tokenize;
use web3::helpers::CallResult;

pub fn raw_query<A, B, C, P, T>(
    eth: Eth<T>,
    abi: &ethabi::Contract,
    address: C,
    func: &str,
    params: P,
    from: A,
    options: Options,
    block: B,
) -> CallResult<Bytes, T::Out>
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

/// Decode a single ethabi param of type bytes[]
fn decode_bytes_array(bytes: &[u8]) -> Vec<Vec<u8>> {
    let num_elements = U256::from_big_endian(&bytes[0..32]);

    let element_offsets: Vec<U256> = (0..num_elements.as_u64())
        .map(|element_i| {
            let element_data_offset = U256::from_big_endian(
                // additional offset of 1 to account for leading word that holds the number of elements
                &bytes[(32 * (element_i + 1) as usize)..(32 * (element_i + 2) as usize)],
            );
            // + 32 because of leading word
            element_data_offset + 32.into()
        })
        .collect();

    element_offsets
        .into_iter()
        .map(|element_start_offset| {
            let element_length = U256::from_big_endian(
                &bytes[(element_start_offset.as_u64() as usize)
                           ..((element_start_offset.as_u64() + 32) as usize)],
            );
            bytes[((element_start_offset.as_u64() + 32) as usize)
                      ..((element_start_offset + element_length).as_u64() as usize + 32)]
                .to_owned()
        })
        .collect()
}

pub fn decode_class_call_output(bytes: &[u8]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let first_param_offset = U256::from_big_endian(&bytes[0..32]);
    let second_param_offset = U256::from_big_endian(&bytes[32..64]);

    let first_param = decode_bytes_array(
        &bytes[(first_param_offset.as_u64() as usize)..(second_param_offset.as_u64() as usize)],
    );
    let second_param =
        decode_bytes_array(&bytes[(second_param_offset.as_u64() as usize)..bytes.len()]);

    let decoded = (first_param, second_param);

    decoded
}

pub fn decode_individual_call_output(bytes: &[u8]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let first_param_offset = U256::from_big_endian(&bytes[0..32]);
    let second_param_offset = U256::from_big_endian(&bytes[32..64]);
    let third_param_offset = U256::from_big_endian(&bytes[64..96]);

    let first_param = decode_bytes_array(
        &bytes[(first_param_offset.as_u64() as usize)..(second_param_offset.as_u64() as usize)],
    );
    let second_param = decode_bytes_array(
        &bytes[(second_param_offset.as_u64() as usize)..(third_param_offset.as_u64() as usize)],
    );
    let third_param =
        decode_bytes_array(&bytes[(third_param_offset.as_u64() as usize)..bytes.len()]);

    let decoded = (first_param, second_param, third_param);

    decoded
}
