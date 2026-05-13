use std::str::FromStr as _;

use alloy_primitives::Address as AlloyAddress;
use alloy_sol_types::{sol, SolCall};
use policy_engine::action::Address;

use crate::{
    CallMatchKey, DecodeContext, DecodedArg, DecodedCall, DecodedValue, Decoder, DecoderError,
    DecoderId,
};

pub const UNISWAP_V3_DECODER_ID: &str = "uniswap_v3";
pub const SWAP_ROUTER_MAINNET: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";

pub const EXACT_INPUT_SINGLE_SELECTOR: [u8; 4] = [0x41, 0x4b, 0xf3, 0x89];
pub const EXACT_INPUT_SELECTOR: [u8; 4] = [0xc0, 0x4b, 0x8d, 0x59];

const EXACT_INPUT_SINGLE_SIGNATURE: &str =
    "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))";
const EXACT_INPUT_SIGNATURE: &str = "exactInput((bytes,address,uint256,uint256,uint256))";

sol! {
    struct SolExactInputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24 fee;
        address recipient;
        uint256 deadline;
        uint256 amountIn;
        uint256 amountOutMinimum;
        uint160 sqrtPriceLimitX96;
    }

    function exactInputSingle(SolExactInputSingleParams params) external payable returns (uint256 amountOut);

    struct SolExactInputParams {
        bytes path;
        address recipient;
        uint256 deadline;
        uint256 amountIn;
        uint256 amountOutMinimum;
    }

    function exactInput(SolExactInputParams params) external payable returns (uint256 amountOut);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UniswapV3Decoder;

impl UniswapV3Decoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for UniswapV3Decoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(UNISWAP_V3_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![
            mainnet_match_key(EXACT_INPUT_SINGLE_SELECTOR),
            mainnet_match_key(EXACT_INPUT_SELECTOR),
        ]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        let selector = selector(calldata)?;
        match selector {
            EXACT_INPUT_SINGLE_SELECTOR => decode_exact_input_single(calldata),
            EXACT_INPUT_SELECTOR => decode_exact_input(calldata),
            _ => Err(DecoderError::UnsupportedSelector),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExactInputSingleDecoder;

impl ExactInputSingleDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for ExactInputSingleDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(UNISWAP_V3_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(EXACT_INPUT_SINGLE_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, EXACT_INPUT_SINGLE_SELECTOR)?;
        decode_exact_input_single(calldata)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExactInputDecoder;

impl ExactInputDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for ExactInputDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(UNISWAP_V3_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(EXACT_INPUT_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, EXACT_INPUT_SELECTOR)?;
        decode_exact_input(calldata)
    }
}

fn decode_exact_input_single(calldata: &[u8]) -> Result<DecodedCall, DecoderError> {
    let call = exactInputSingleCall::abi_decode(calldata, true)
        .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;
    let fee = u32::try_from(call.params.fee)
        .map_err(|e| DecoderError::AbiMismatch(format!("fee out of range: {e}")))?;

    Ok(DecodedCall {
        decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
        function_signature: EXACT_INPUT_SINGLE_SIGNATURE.to_owned(),
        args: vec![
            address_arg("tokenIn", call.params.tokenIn)?,
            address_arg("tokenOut", call.params.tokenOut)?,
            uint_arg_typed("fee", "uint24", alloy_primitives::U256::from(fee as u64)),
            address_arg("recipient", call.params.recipient)?,
            uint_arg("deadline", call.params.deadline),
            uint_arg("amountIn", call.params.amountIn),
            uint_arg("amountOutMinimum", call.params.amountOutMinimum),
            uint_arg_typed(
                "sqrtPriceLimitX96",
                "uint160",
                alloy_primitives::U256::from_be_slice(
                    &call.params.sqrtPriceLimitX96.to_be_bytes::<20>(),
                ),
            ),
        ],
        nested: vec![],
    })
}

fn decode_exact_input(calldata: &[u8]) -> Result<DecodedCall, DecoderError> {
    let call = exactInputCall::abi_decode(calldata, true)
        .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

    Ok(DecodedCall {
        decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
        function_signature: EXACT_INPUT_SIGNATURE.to_owned(),
        args: vec![
            bytes_arg("path", call.params.path.to_vec()),
            address_arg("recipient", call.params.recipient)?,
            uint_arg("deadline", call.params.deadline),
            uint_arg("amountIn", call.params.amountIn),
            uint_arg("amountOutMinimum", call.params.amountOutMinimum),
        ],
        nested: vec![],
    })
}

fn mainnet_match_key(selector: [u8; 4]) -> CallMatchKey {
    CallMatchKey {
        chain_id: 1,
        to: Address::from_str(SWAP_ROUTER_MAINNET)
            .expect("static Uniswap V3 mainnet router address must be valid"),
        selector,
    }
}

fn selector(calldata: &[u8]) -> Result<[u8; 4], DecoderError> {
    calldata
        .get(..4)
        .ok_or_else(|| DecoderError::InvalidCalldata("calldata shorter than selector".to_owned()))?
        .try_into()
        .map_err(|_| DecoderError::InvalidCalldata("calldata shorter than selector".to_owned()))
}

fn ensure_selector(calldata: &[u8], expected: [u8; 4]) -> Result<(), DecoderError> {
    if selector(calldata)? == expected {
        Ok(())
    } else {
        Err(DecoderError::UnsupportedSelector)
    }
}

fn uint_arg(name: &str, value: alloy_primitives::U256) -> DecodedArg {
    uint_arg_typed(name, "uint256", value)
}

fn uint_arg_typed(name: &str, abi_type: &str, value: alloy_primitives::U256) -> DecodedArg {
    DecodedArg {
        name: name.to_owned(),
        abi_type: abi_type.to_owned(),
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

fn address_arg(name: &str, value: AlloyAddress) -> Result<DecodedArg, DecoderError> {
    Ok(DecodedArg {
        name: name.to_owned(),
        abi_type: "address".to_owned(),
        value: DecodedValue::Address(policy_address(value)?),
    })
}

fn policy_address(value: AlloyAddress) -> Result<Address, DecoderError> {
    Address::from_str(&format!("0x{}", hex::encode(value.0))).map_err(|e| {
        DecoderError::Internal(anyhow::anyhow!(
            "decoded address failed policy address validation: {e}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ExactInputDecoder, ExactInputSingleDecoder, UniswapV3Decoder, EXACT_INPUT_SELECTOR,
        EXACT_INPUT_SINGLE_SELECTOR, SWAP_ROUTER_MAINNET, UNISWAP_V3_DECODER_ID,
    };
    use crate::{DecodeContext, DecodedValue, Decoder as _, DecoderId};
    use alloy_primitives::U256;
    use policy_engine::action::{Address, DecimalString};
    use serde::Deserialize;
    use std::str::FromStr as _;

    #[derive(Deserialize)]
    struct Fixture {
        chain_id: u64,
        rpc: Rpc,
    }

    #[derive(Deserialize)]
    struct Rpc {
        params: Vec<TxParam>,
    }

    #[derive(Deserialize)]
    struct TxParam {
        to: String,
        data: String,
    }

    fn fixture(input: &str) -> (Fixture, Vec<u8>) {
        let fixture: Fixture = serde_json::from_str(input).unwrap();
        let data = fixture.rpc.params[0]
            .data
            .strip_prefix("0x")
            .unwrap()
            .to_owned();
        (fixture, hex::decode(data).unwrap())
    }

    fn context<'a>(
        fixture: &'a Fixture,
        to: &'a Address,
        value: &'a DecimalString,
    ) -> DecodeContext<'a> {
        DecodeContext {
            chain_id: fixture.chain_id,
            to,
            value,
            block_timestamp: Some(1_700_000_000),
        }
    }

    fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn arg<'a>(decoded: &'a crate::DecodedCall, name: &str) -> &'a DecodedValue {
        &decoded
            .args
            .iter()
            .find(|arg| arg.name == name)
            .unwrap()
            .value
    }

    #[test]
    fn test_decode_exact_input_single() {
        let (fixture, calldata) = fixture(include_str!(
            "../../../../integration-tests/data/golden/inputs/swap_uniswap_v3_exact_input_single.json"
        ));
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("0");

        let decoded = ExactInputSingleDecoder::new()
            .decode(&context(&fixture, &to, &value), &calldata)
            .unwrap();

        assert_eq!(decoded.decoder_id, DecoderId::new(UNISWAP_V3_DECODER_ID));
        assert_eq!(
            decoded.function_signature,
            "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 8);
        assert_eq!(
            arg(&decoded, "tokenIn"),
            &DecodedValue::Address(address("0xdac17f958d2ee523a2206206994597c13d831ec7"))
        );
        assert_eq!(
            arg(&decoded, "tokenOut"),
            &DecodedValue::Address(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
        );
        assert_eq!(
            arg(&decoded, "fee"),
            &DecodedValue::Uint(U256::from(3000u64))
        );
        assert_eq!(
            arg(&decoded, "recipient"),
            &DecodedValue::Address(address("0x1111111111111111111111111111111111111111"))
        );
        assert_eq!(
            arg(&decoded, "deadline"),
            &DecodedValue::Uint(U256::from(9_999_999_999u64))
        );
        assert_eq!(
            arg(&decoded, "amountIn"),
            &DecodedValue::Uint(U256::from(200_000_000u64))
        );
        assert_eq!(
            arg(&decoded, "amountOutMinimum"),
            &DecodedValue::Uint(U256::ZERO)
        );
        assert_eq!(
            arg(&decoded, "sqrtPriceLimitX96"),
            &DecodedValue::Uint(U256::ZERO)
        );
    }

    #[test]
    fn test_decode_exact_input() {
        let (fixture, calldata) = fixture(include_str!(
            "../../../../integration-tests/data/golden/inputs/swap_uniswap_v3_exact_input_multi.json"
        ));
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("0");

        let decoded = ExactInputDecoder::new()
            .decode(&context(&fixture, &to, &value), &calldata)
            .unwrap();

        assert_eq!(decoded.decoder_id, DecoderId::new(UNISWAP_V3_DECODER_ID));
        assert_eq!(
            decoded.function_signature,
            "exactInput((bytes,address,uint256,uint256,uint256))"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 5);
        assert_eq!(decoded.args[0].name, "path");
        assert_eq!(decoded.args[0].abi_type, "bytes");
        assert_eq!(
            arg(&decoded, "path"),
            &DecodedValue::Bytes(
                hex::decode(
                    "dac17f958d2ee523a2206206994597c13d831ec70001f4a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000bb8c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                )
                .unwrap()
            )
        );
        assert_eq!(
            arg(&decoded, "recipient"),
            &DecodedValue::Address(address("0x1111111111111111111111111111111111111111"))
        );
        assert_eq!(arg(&decoded, "deadline"), &DecodedValue::Uint(U256::ONE));
        assert_eq!(
            arg(&decoded, "amountIn"),
            &DecodedValue::Uint(U256::from(1_000_000u64))
        );
        assert_eq!(
            arg(&decoded, "amountOutMinimum"),
            &DecodedValue::Uint(U256::ZERO)
        );
    }

    #[test]
    fn test_match_keys_cover_mainnet_router() {
        let router = address(SWAP_ROUTER_MAINNET);

        assert!(ExactInputSingleDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router.clone(),
                selector: EXACT_INPUT_SINGLE_SELECTOR,
            }));
        assert!(ExactInputDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router.clone(),
                selector: EXACT_INPUT_SELECTOR,
            }));

        let keys = UniswapV3Decoder::new().match_keys();
        assert!(keys.contains(&crate::CallMatchKey {
            chain_id: 1,
            to: router.clone(),
            selector: EXACT_INPUT_SINGLE_SELECTOR,
        }));
        assert!(keys.contains(&crate::CallMatchKey {
            chain_id: 1,
            to: router,
            selector: EXACT_INPUT_SELECTOR,
        }));
    }
}
