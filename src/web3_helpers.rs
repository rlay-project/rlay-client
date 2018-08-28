use ethabi;
use rlay_ontology::ontology::{Annotation, Class, ClassAssertion, Individual,
                              NegativeClassAssertion};
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

/// Decode a single ethabi param of type bytes
fn decode_bytes(bytes: &[u8]) -> Vec<u8> {
    let length = U256::from_big_endian(&bytes[0..32]);
    bytes[((32) as usize)..((length).as_u64() as usize + 32)].to_owned()
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
            decode_bytes(&bytes[(element_start_offset.as_u64() as usize)..bytes.len()])
        })
        .collect()
}

pub trait FromABIV2Response {
    fn from_abiv2(bytes: &[u8]) -> Self;
}

macro_rules! decode_offset {
    ($bytes_var:ident, $offset_var:ident, $start:expr, $end:expr) => (
        let $offset_var = U256::from_big_endian(&$bytes_var[$start..$end]);
    );
}

macro_rules! decode_param {
    (bytes_array; $bytes_var:ident, $param_var:ident, $start:expr, $end:expr) => (
        let $param_var = decode_bytes_array(
            &$bytes_var[($start.as_u64() as usize)..($end.as_u64() as usize)],
        );
    );
    (bytes_array; $bytes_var:ident, $param_var:ident, $start:expr) => (
        let $param_var = decode_bytes_array(
            &$bytes_var[($start.as_u64() as usize)..$bytes_var.len()],
        );
    );
    (bytes; $bytes_var:ident, $param_var:ident, $start:expr, $end:expr) => (
        let $param_var = decode_bytes(
            &$bytes_var[($start.as_u64() as usize)..($end.as_u64() as usize)],
        );
    );
    (bytes; $bytes_var:ident, $param_var:ident, $start:expr) => (
        let $param_var = decode_bytes(
            &$bytes_var[($start.as_u64() as usize)..$bytes_var.len()],
        );
    );
}

impl FromABIV2Response for Class {
    fn from_abiv2(bytes: &[u8]) -> Self {
        decode_offset!(bytes, annotations_offset, 0, 32);
        decode_offset!(bytes, super_class_expression_offset, 32, 64);

        decode_param!(
            bytes_array; bytes,
            annotations,
            annotations_offset,
            super_class_expression_offset
        );
        decode_param!(
            bytes_array; bytes,
            super_class_expression,
            super_class_expression_offset
        );

        Self {
            annotations,
            super_class_expression,
        }
    }
}

impl FromABIV2Response for Individual {
    fn from_abiv2(bytes: &[u8]) -> Self {
        decode_offset!(bytes, annotations_offset, 0, 32);

        decode_param!(bytes_array; bytes, annotations, annotations_offset);

        Self { annotations }
    }
}

impl FromABIV2Response for Annotation {
    fn from_abiv2(bytes: &[u8]) -> Self {
        decode_offset!(bytes, annotations_offset, 0, 32);
        decode_offset!(bytes, property_offset, 32, 64);
        decode_offset!(bytes, value_offset, 64, 96);

        decode_param!(
            bytes_array;
            bytes,
            annotations,
            annotations_offset,
            property_offset
        );
        decode_param!(
            bytes;
            bytes,
            property,
            property_offset,
            value_offset
        );
        decode_param!(
            bytes; bytes,
            value,
            value_offset
        );

        Self {
            annotations,
            property,
            value,
        }
    }
}

impl FromABIV2Response for ClassAssertion {
    fn from_abiv2(bytes: &[u8]) -> Self {
        decode_offset!(bytes, annotations_offset, 0, 32);
        decode_offset!(bytes, class_offset, 32, 64);
        decode_offset!(bytes, subject_offset, 64, 96);

        decode_param!(
            bytes_array;
            bytes,
            annotations,
            annotations_offset,
            class_offset
        );
        decode_param!(
            bytes;
            bytes,
            class,
            class_offset,
            subject_offset
        );
        decode_param!(
            bytes; bytes,
            subject,
            subject_offset
        );

        Self {
            annotations,
            class,
            subject,
        }
    }
}

impl FromABIV2Response for NegativeClassAssertion {
    fn from_abiv2(bytes: &[u8]) -> Self {
        decode_offset!(bytes, annotations_offset, 0, 32);
        decode_offset!(bytes, class_offset, 32, 64);
        decode_offset!(bytes, subject_offset, 64, 96);

        decode_param!(
            bytes_array;
            bytes,
            annotations,
            annotations_offset,
            class_offset
        );
        decode_param!(
            bytes;
            bytes,
            class,
            class_offset,
            subject_offset
        );
        decode_param!(
            bytes; bytes,
            subject,
            subject_offset
        );

        Self {
            annotations,
            class,
            subject,
        }
    }
}
