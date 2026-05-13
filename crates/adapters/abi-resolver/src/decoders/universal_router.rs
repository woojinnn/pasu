use std::str::FromStr as _;

use alloy_sol_types::{sol, SolType};
use policy_engine::action::Address;

use crate::{
    CallMatchKey, DecodeContext, DecodedArg, DecodedCall, DecodedValue, Decoder, DecoderError,
    DecoderId,
};

pub const UNIVERSAL_ROUTER_EXECUTE_DECODER_ID: &str = "universal-router/execute";
pub const UNIVERSAL_ROUTER_EXECUTE_WITH_DEADLINE_DECODER_ID: &str =
    "universal-router/executeWithDeadline";

pub const EXECUTE_SELECTOR: [u8; 4] = [0x24, 0x85, 0x6b, 0xc3];
pub const EXECUTE_WITH_DEADLINE_SELECTOR: [u8; 4] = [0x35, 0x93, 0x56, 0x4c];

const EXECUTE_SIGNATURE: &str = "execute(bytes,bytes[])";
const EXECUTE_WITH_DEADLINE_SIGNATURE: &str = "execute(bytes,bytes[],uint256)";

const UNIVERSAL_ROUTER_CHAIN_IDS: [u64; 5] = [1, 8453, 10, 42161, 137];
const UNIVERSAL_ROUTER_ADDRESSES: [&str; 4] = [
    "0x66a9893cc07d91d95644aedd05d03f95e1dba8af",
    "0x4c82d1fbfe28c977cbb58d8c7ff8fcf9f70a2cca",
    "0x3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad",
    "0xef1c6e67703c7bd7107eed8303fbe6ec2554bf6b",
];

type ExecuteInput = sol! { (bytes, bytes[]) };
type ExecuteWithDeadlineInput = sol! { (bytes, bytes[], uint256) };

#[derive(Debug, Clone, Copy, Default)]
pub struct UniversalRouterDecoder;

impl UniversalRouterDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for UniversalRouterDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(UNIVERSAL_ROUTER_EXECUTE_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        UNIVERSAL_ROUTER_CHAIN_IDS
            .into_iter()
            .flat_map(|chain_id| {
                UNIVERSAL_ROUTER_ADDRESSES.into_iter().flat_map(move |to| {
                    [EXECUTE_SELECTOR, EXECUTE_WITH_DEADLINE_SELECTOR].map(move |selector| {
                        CallMatchKey {
                            chain_id,
                            to: Address::from_str(to)
                                .expect("static Universal Router address must be valid"),
                            selector,
                        }
                    })
                })
            })
            .collect()
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        let selector = selector(calldata)?;
        match selector {
            EXECUTE_SELECTOR => decode_execute(calldata),
            EXECUTE_WITH_DEADLINE_SELECTOR => decode_execute_with_deadline(calldata),
            _ => Err(DecoderError::UnsupportedSelector),
        }
    }
}

fn decode_execute(calldata: &[u8]) -> Result<DecodedCall, DecoderError> {
    let (commands, inputs) = ExecuteInput::abi_decode_sequence(payload(calldata)?, true)
        .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

    Ok(DecodedCall {
        decoder_id: DecoderId::new(UNIVERSAL_ROUTER_EXECUTE_DECODER_ID),
        function_signature: EXECUTE_SIGNATURE.to_owned(),
        args: vec![
            bytes_arg("commands", commands.to_vec()),
            bytes_array_arg(
                "inputs",
                inputs.into_iter().map(|input| input.to_vec()).collect(),
            ),
        ],
        nested: vec![],
    })
}

fn decode_execute_with_deadline(calldata: &[u8]) -> Result<DecodedCall, DecoderError> {
    let (commands, inputs, deadline) =
        ExecuteWithDeadlineInput::abi_decode_sequence(payload(calldata)?, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

    Ok(DecodedCall {
        decoder_id: DecoderId::new(UNIVERSAL_ROUTER_EXECUTE_WITH_DEADLINE_DECODER_ID),
        function_signature: EXECUTE_WITH_DEADLINE_SIGNATURE.to_owned(),
        args: vec![
            bytes_arg("commands", commands.to_vec()),
            bytes_array_arg(
                "inputs",
                inputs.into_iter().map(|input| input.to_vec()).collect(),
            ),
            uint_arg("deadline", deadline),
        ],
        nested: vec![],
    })
}

fn selector(calldata: &[u8]) -> Result<[u8; 4], DecoderError> {
    calldata
        .get(..4)
        .ok_or_else(|| DecoderError::InvalidCalldata("calldata shorter than selector".to_owned()))?
        .try_into()
        .map_err(|_| DecoderError::InvalidCalldata("calldata shorter than selector".to_owned()))
}

fn payload(calldata: &[u8]) -> Result<&[u8], DecoderError> {
    calldata
        .get(4..)
        .ok_or_else(|| DecoderError::InvalidCalldata("calldata shorter than selector".to_owned()))
}

fn uint_arg(name: &str, value: alloy_primitives::U256) -> DecodedArg {
    DecodedArg {
        name: name.to_owned(),
        abi_type: "uint256".to_owned(),
        value: DecodedValue::Uint(value),
    }
}

fn bytes_arg(name: &str, value: Vec<u8>) -> DecodedArg {
    DecodedArg {
        name: name.to_owned(),
        abi_type: "bytes".to_owned(),
        value: DecodedValue::Bytes(value),
    }
}

fn bytes_array_arg(name: &str, values: Vec<Vec<u8>>) -> DecodedArg {
    DecodedArg {
        name: name.to_owned(),
        abi_type: "bytes[]".to_owned(),
        value: DecodedValue::Array(values.into_iter().map(DecodedValue::Bytes).collect()),
    }
}
