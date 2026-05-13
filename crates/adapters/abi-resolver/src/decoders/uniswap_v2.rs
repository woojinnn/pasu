use std::str::FromStr as _;

use alloy_primitives::Address as AlloyAddress;
use alloy_sol_types::{sol, SolCall};
use policy_engine::action::Address;

use crate::{
    CallMatchKey, DecodeContext, DecodedArg, DecodedCall, DecodedValue, Decoder, DecoderError,
    DecoderId,
};

pub const UNISWAP_V2_ROUTER_MAINNET: &str = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D";

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID: &str = "uniswap-v2/swapExactTokensForTokens";
pub const SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID: &str = "uniswap-v2/swapTokensForExactTokens";
pub const SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID: &str = "uniswap-v2/swapExactETHForTokens";
pub const SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID: &str = "uniswap-v2/swapTokensForExactETH";
pub const SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID: &str = "uniswap-v2/swapExactTokensForETH";
pub const SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID: &str = "uniswap-v2/swapETHForExactTokens";

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR: [u8; 4] = [0x38, 0xed, 0x17, 0x39];
pub const SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR: [u8; 4] = [0x88, 0x03, 0xdb, 0xee];
pub const SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR: [u8; 4] = [0x7f, 0xf3, 0x6a, 0xb5];
pub const SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR: [u8; 4] = [0x4a, 0x25, 0xd9, 0x4a];
pub const SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR: [u8; 4] = [0x18, 0xcb, 0xaf, 0xe5];
pub const SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR: [u8; 4] = [0xfb, 0x3b, 0xdb, 0x41];

const SWAP_EXACT_TOKENS_FOR_TOKENS_SIGNATURE: &str =
    "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)";
const SWAP_TOKENS_FOR_EXACT_TOKENS_SIGNATURE: &str =
    "swapTokensForExactTokens(uint256,uint256,address[],address,uint256)";
const SWAP_EXACT_ETH_FOR_TOKENS_SIGNATURE: &str =
    "swapExactETHForTokens(uint256,address[],address,uint256)";
const SWAP_TOKENS_FOR_EXACT_ETH_SIGNATURE: &str =
    "swapTokensForExactETH(uint256,uint256,address[],address,uint256)";
const SWAP_EXACT_TOKENS_FOR_ETH_SIGNATURE: &str =
    "swapExactTokensForETH(uint256,uint256,address[],address,uint256)";
const SWAP_ETH_FOR_EXACT_TOKENS_SIGNATURE: &str =
    "swapETHForExactTokens(uint256,address[],address,uint256)";

sol! {
    function swapExactTokensForTokens(
        uint256 amountIn,
        uint256 amountOutMin,
        address[] path,
        address to,
        uint256 deadline
    ) external returns (uint256[] amounts);

    function swapTokensForExactTokens(
        uint256 amountOut,
        uint256 amountInMax,
        address[] path,
        address to,
        uint256 deadline
    ) external returns (uint256[] amounts);

    function swapExactETHForTokens(
        uint256 amountOutMin,
        address[] path,
        address to,
        uint256 deadline
    ) external payable returns (uint256[] amounts);

    function swapTokensForExactETH(
        uint256 amountOut,
        uint256 amountInMax,
        address[] path,
        address to,
        uint256 deadline
    ) external returns (uint256[] amounts);

    function swapExactTokensForETH(
        uint256 amountIn,
        uint256 amountOutMin,
        address[] path,
        address to,
        uint256 deadline
    ) external returns (uint256[] amounts);

    function swapETHForExactTokens(
        uint256 amountOut,
        address[] path,
        address to,
        uint256 deadline
    ) external payable returns (uint256[] amounts);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapExactTokensForTokensDecoder;

impl SwapExactTokensForTokensDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for SwapExactTokensForTokensDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR)?;
        let call = swapExactTokensForTokensCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: SWAP_EXACT_TOKENS_FOR_TOKENS_SIGNATURE.to_owned(),
            args: vec![
                uint_arg("amountIn", call.amountIn),
                uint_arg("amountOutMin", call.amountOutMin),
                address_array_arg("path", call.path)?,
                address_arg("to", call.to)?,
                uint_arg("deadline", call.deadline),
            ],
            nested: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapTokensForExactTokensDecoder;

impl SwapTokensForExactTokensDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for SwapTokensForExactTokensDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR)?;
        let call = swapTokensForExactTokensCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: SWAP_TOKENS_FOR_EXACT_TOKENS_SIGNATURE.to_owned(),
            args: vec![
                uint_arg("amountOut", call.amountOut),
                uint_arg("amountInMax", call.amountInMax),
                address_array_arg("path", call.path)?,
                address_arg("to", call.to)?,
                uint_arg("deadline", call.deadline),
            ],
            nested: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapExactETHForTokensDecoder;

impl SwapExactETHForTokensDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for SwapExactETHForTokensDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR)?;
        let call = swapExactETHForTokensCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: SWAP_EXACT_ETH_FOR_TOKENS_SIGNATURE.to_owned(),
            args: vec![
                uint_arg("amountOutMin", call.amountOutMin),
                address_array_arg("path", call.path)?,
                address_arg("to", call.to)?,
                uint_arg("deadline", call.deadline),
            ],
            nested: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapTokensForExactETHDecoder;

impl SwapTokensForExactETHDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for SwapTokensForExactETHDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR)?;
        let call = swapTokensForExactETHCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: SWAP_TOKENS_FOR_EXACT_ETH_SIGNATURE.to_owned(),
            args: vec![
                uint_arg("amountOut", call.amountOut),
                uint_arg("amountInMax", call.amountInMax),
                address_array_arg("path", call.path)?,
                address_arg("to", call.to)?,
                uint_arg("deadline", call.deadline),
            ],
            nested: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapExactTokensForETHDecoder;

impl SwapExactTokensForETHDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for SwapExactTokensForETHDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR)?;
        let call = swapExactTokensForETHCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: SWAP_EXACT_TOKENS_FOR_ETH_SIGNATURE.to_owned(),
            args: vec![
                uint_arg("amountIn", call.amountIn),
                uint_arg("amountOutMin", call.amountOutMin),
                address_array_arg("path", call.path)?,
                address_arg("to", call.to)?,
                uint_arg("deadline", call.deadline),
            ],
            nested: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapETHForExactTokensDecoder;

impl SwapETHForExactTokensDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Decoder for SwapETHForExactTokensDecoder {
    fn id(&self) -> DecoderId {
        DecoderId::new(SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        vec![mainnet_match_key(SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR)]
    }

    fn decode(
        &self,
        _ctx: &DecodeContext<'_>,
        calldata: &[u8],
    ) -> Result<DecodedCall, DecoderError> {
        ensure_selector(calldata, SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR)?;
        let call = swapETHForExactTokensCall::abi_decode(calldata, true)
            .map_err(|e| DecoderError::AbiMismatch(e.to_string()))?;

        Ok(DecodedCall {
            decoder_id: self.id(),
            function_signature: SWAP_ETH_FOR_EXACT_TOKENS_SIGNATURE.to_owned(),
            args: vec![
                uint_arg("amountOut", call.amountOut),
                address_array_arg("path", call.path)?,
                address_arg("to", call.to)?,
                uint_arg("deadline", call.deadline),
            ],
            nested: vec![],
        })
    }
}

fn mainnet_match_key(selector: [u8; 4]) -> CallMatchKey {
    CallMatchKey {
        chain_id: 1,
        to: Address::from_str(UNISWAP_V2_ROUTER_MAINNET)
            .expect("static Uniswap V2 mainnet router address must be valid"),
        selector,
    }
}

fn ensure_selector(calldata: &[u8], expected: [u8; 4]) -> Result<(), DecoderError> {
    let selector: [u8; 4] = calldata
        .get(..4)
        .ok_or_else(|| DecoderError::InvalidCalldata("calldata shorter than selector".to_owned()))?
        .try_into()
        .expect("slice length checked");
    if selector == expected {
        Ok(())
    } else {
        Err(DecoderError::UnsupportedSelector)
    }
}

fn uint_arg(name: &str, value: alloy_primitives::U256) -> DecodedArg {
    DecodedArg {
        name: name.to_owned(),
        abi_type: "uint256".to_owned(),
        value: DecodedValue::Uint(value),
    }
}

fn address_arg(name: &str, value: AlloyAddress) -> Result<DecodedArg, DecoderError> {
    Ok(DecodedArg {
        name: name.to_owned(),
        abi_type: "address".to_owned(),
        value: DecodedValue::Address(policy_address(value)?),
    })
}

fn address_array_arg(name: &str, values: Vec<AlloyAddress>) -> Result<DecodedArg, DecoderError> {
    Ok(DecodedArg {
        name: name.to_owned(),
        abi_type: "address[]".to_owned(),
        value: DecodedValue::Array(
            values
                .into_iter()
                .map(policy_address)
                .map(|r| r.map(DecodedValue::Address))
                .collect::<Result<Vec<_>, _>>()?,
        ),
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
        SwapETHForExactTokensDecoder, SwapExactETHForTokensDecoder, SwapExactTokensForETHDecoder,
        SwapExactTokensForTokensDecoder, SwapTokensForExactETHDecoder,
        SwapTokensForExactTokensDecoder, SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR,
        SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR, SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR,
        SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR, SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR,
        SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR, UNISWAP_V2_ROUTER_MAINNET,
    };
    use crate::{DecodeContext, DecodedCall, DecodedValue, Decoder as _, DecoderId};
    use alloy_primitives::{Address as AlloyAddress, U256};
    use alloy_sol_types::{sol, SolCall};
    use policy_engine::action::{Address, DecimalString};
    use serde::Deserialize;
    use std::str::FromStr as _;

    sol! {
        function swapTokensForExactETH(
            uint256 amountOut,
            uint256 amountInMax,
            address[] path,
            address to,
            uint256 deadline
        ) external returns (uint256[] amounts);

        function swapExactTokensForETH(
            uint256 amountIn,
            uint256 amountOutMin,
            address[] path,
            address to,
            uint256 deadline
        ) external returns (uint256[] amounts);

        function swapETHForExactTokens(
            uint256 amountOut,
            address[] path,
            address to,
            uint256 deadline
        ) external payable returns (uint256[] amounts);
    }

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

    fn alloy_address(value: &str) -> AlloyAddress {
        AlloyAddress::from_str(value).unwrap()
    }

    fn encoded_swap_tokens_for_exact_eth() -> Vec<u8> {
        swapTokensForExactETHCall {
            amountOut: U256::from(1_000_000_000_000_000_000u64),
            amountInMax: U256::from(4_000_000_000u64),
            path: vec![
                alloy_address("0xdac17f958d2ee523a2206206994597c13d831ec7"),
                alloy_address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
            ],
            to: alloy_address("0x1111111111111111111111111111111111111111"),
            deadline: U256::from(9_999_999_999u64),
        }
        .abi_encode()
    }

    fn encoded_swap_exact_tokens_for_eth() -> Vec<u8> {
        swapExactTokensForETHCall {
            amountIn: U256::from(200_000_000u64),
            amountOutMin: U256::ZERO,
            path: vec![
                alloy_address("0xdac17f958d2ee523a2206206994597c13d831ec7"),
                alloy_address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
            ],
            to: alloy_address("0x1111111111111111111111111111111111111111"),
            deadline: U256::from(9_999_999_999u64),
        }
        .abi_encode()
    }

    fn encoded_swap_eth_for_exact_tokens() -> Vec<u8> {
        swapETHForExactTokensCall {
            amountOut: U256::from(200_000_000u64),
            path: vec![
                alloy_address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
                alloy_address("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            ],
            to: alloy_address("0x1111111111111111111111111111111111111111"),
            deadline: U256::from(9_999_999_999u64),
        }
        .abi_encode()
    }

    fn assert_uint_arg(decoded: &DecodedCall, index: usize, name: &str, expected: U256) {
        assert_eq!(decoded.args[index].name, name);
        assert_eq!(decoded.args[index].abi_type, "uint256");
        assert_eq!(decoded.args[index].value, DecodedValue::Uint(expected));
    }

    fn assert_address_arg(decoded: &DecodedCall, index: usize, name: &str, expected: &str) {
        assert_eq!(decoded.args[index].name, name);
        assert_eq!(decoded.args[index].abi_type, "address");
        assert_eq!(
            decoded.args[index].value,
            DecodedValue::Address(address(expected))
        );
    }

    fn assert_path_arg(decoded: &DecodedCall, index: usize, expected: &[&str]) {
        assert_eq!(decoded.args[index].name, "path");
        assert_eq!(decoded.args[index].abi_type, "address[]");
        assert_eq!(
            decoded.args[index].value,
            DecodedValue::Array(
                expected
                    .iter()
                    .map(|value| DecodedValue::Address(address(value)))
                    .collect()
            )
        );
    }

    #[test]
    fn test_decode_swap_exact_tokens_for_tokens() {
        let (fixture, calldata) = fixture(include_str!(
            "../../../../integration-tests/data/golden/inputs/swap_uniswap_v2_exact_in.json"
        ));
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("0");

        let decoded = SwapExactTokensForTokensDecoder::new()
            .decode(&context(&fixture, &to, &value), &calldata)
            .unwrap();

        assert_eq!(
            decoded.decoder_id,
            DecoderId::new("uniswap-v2/swapExactTokensForTokens")
        );
        assert_eq!(
            decoded.function_signature,
            "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 5);
        assert_eq!(decoded.args[0].name, "amountIn");
        assert_eq!(decoded.args[0].abi_type, "uint256");
        assert_eq!(
            decoded.args[0].value,
            DecodedValue::Uint(U256::from(200_000_000u64))
        );
        assert_eq!(decoded.args[1].name, "amountOutMin");
        assert_eq!(decoded.args[1].value, DecodedValue::Uint(U256::ZERO));
        assert_eq!(decoded.args[2].name, "path");
        assert_eq!(decoded.args[2].abi_type, "address[]");
        assert_eq!(
            decoded.args[2].value,
            DecodedValue::Array(vec![
                DecodedValue::Address(address("0xdac17f958d2ee523a2206206994597c13d831ec7")),
                DecodedValue::Address(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")),
            ])
        );
        assert_eq!(decoded.args[3].name, "to");
        assert_eq!(
            decoded.args[3].value,
            DecodedValue::Address(address("0x1111111111111111111111111111111111111111"))
        );
        assert_eq!(decoded.args[4].name, "deadline");
        assert_eq!(
            decoded.args[4].value,
            DecodedValue::Uint(U256::from(9_999_999_999u64))
        );
    }

    #[test]
    fn test_decode_swap_tokens_for_exact_tokens() {
        let (fixture, calldata) = fixture(include_str!(
            "../../../../integration-tests/data/golden/inputs/swap_uniswap_v2_exact_out.json"
        ));
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("0");

        let decoded = SwapTokensForExactTokensDecoder::new()
            .decode(&context(&fixture, &to, &value), &calldata)
            .unwrap();

        assert_eq!(
            decoded.decoder_id,
            DecoderId::new("uniswap-v2/swapTokensForExactTokens")
        );
        assert_eq!(
            decoded.function_signature,
            "swapTokensForExactTokens(uint256,uint256,address[],address,uint256)"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 5);
        assert_eq!(decoded.args[0].name, "amountOut");
        assert_eq!(
            decoded.args[0].value,
            DecodedValue::Uint(U256::from(1_000_000_000_000_000_000u64))
        );
        assert_eq!(decoded.args[1].name, "amountInMax");
        assert_eq!(
            decoded.args[1].value,
            DecodedValue::Uint(U256::from(4_000_000_000u64))
        );
        assert_eq!(
            decoded.args[2].value,
            DecodedValue::Array(vec![
                DecodedValue::Address(address("0xdac17f958d2ee523a2206206994597c13d831ec7")),
                DecodedValue::Address(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")),
            ])
        );
        assert_eq!(
            decoded.args[3].value,
            DecodedValue::Address(address("0x1111111111111111111111111111111111111111"))
        );
        assert_eq!(
            decoded.args[4].value,
            DecodedValue::Uint(U256::from(9_999_999_999u64))
        );
    }

    #[test]
    fn test_decode_swap_exact_eth_for_tokens() {
        let (fixture, calldata) = fixture(include_str!(
            "../../../../integration-tests/data/golden/inputs/swap_uniswap_v2_exact_eth_for_tokens.json"
        ));
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("1000000000000000000");

        let decoded = SwapExactETHForTokensDecoder::new()
            .decode(&context(&fixture, &to, &value), &calldata)
            .unwrap();

        assert_eq!(
            decoded.decoder_id,
            DecoderId::new("uniswap-v2/swapExactETHForTokens")
        );
        assert_eq!(
            decoded.function_signature,
            "swapExactETHForTokens(uint256,address[],address,uint256)"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 4);
        assert_uint_arg(&decoded, 0, "amountOutMin", U256::from(200_000_000u64));
        assert_path_arg(
            &decoded,
            1,
            &[
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                "0xdac17f958d2ee523a2206206994597c13d831ec7",
            ],
        );
        assert_address_arg(
            &decoded,
            2,
            "to",
            "0x1111111111111111111111111111111111111111",
        );
        assert_uint_arg(&decoded, 3, "deadline", U256::from(9_999_999_999u64));
    }

    #[test]
    fn test_decode_swap_tokens_for_exact_eth() {
        let fixture = Fixture {
            chain_id: 1,
            rpc: Rpc {
                params: vec![TxParam {
                    to: UNISWAP_V2_ROUTER_MAINNET.to_owned(),
                    data: String::new(),
                }],
            },
        };
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("0");

        let decoded = SwapTokensForExactETHDecoder::new()
            .decode(
                &context(&fixture, &to, &value),
                &encoded_swap_tokens_for_exact_eth(),
            )
            .unwrap();

        assert_eq!(
            decoded.decoder_id,
            DecoderId::new("uniswap-v2/swapTokensForExactETH")
        );
        assert_eq!(
            decoded.function_signature,
            "swapTokensForExactETH(uint256,uint256,address[],address,uint256)"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 5);
        assert_uint_arg(
            &decoded,
            0,
            "amountOut",
            U256::from(1_000_000_000_000_000_000u64),
        );
        assert_uint_arg(&decoded, 1, "amountInMax", U256::from(4_000_000_000u64));
        assert_path_arg(
            &decoded,
            2,
            &[
                "0xdac17f958d2ee523a2206206994597c13d831ec7",
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            ],
        );
        assert_address_arg(
            &decoded,
            3,
            "to",
            "0x1111111111111111111111111111111111111111",
        );
        assert_uint_arg(&decoded, 4, "deadline", U256::from(9_999_999_999u64));
    }

    #[test]
    fn test_decode_swap_exact_tokens_for_eth() {
        let fixture = Fixture {
            chain_id: 1,
            rpc: Rpc {
                params: vec![TxParam {
                    to: UNISWAP_V2_ROUTER_MAINNET.to_owned(),
                    data: String::new(),
                }],
            },
        };
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("0");

        let decoded = SwapExactTokensForETHDecoder::new()
            .decode(
                &context(&fixture, &to, &value),
                &encoded_swap_exact_tokens_for_eth(),
            )
            .unwrap();

        assert_eq!(
            decoded.decoder_id,
            DecoderId::new("uniswap-v2/swapExactTokensForETH")
        );
        assert_eq!(
            decoded.function_signature,
            "swapExactTokensForETH(uint256,uint256,address[],address,uint256)"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 5);
        assert_uint_arg(&decoded, 0, "amountIn", U256::from(200_000_000u64));
        assert_uint_arg(&decoded, 1, "amountOutMin", U256::ZERO);
        assert_path_arg(
            &decoded,
            2,
            &[
                "0xdac17f958d2ee523a2206206994597c13d831ec7",
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            ],
        );
        assert_address_arg(
            &decoded,
            3,
            "to",
            "0x1111111111111111111111111111111111111111",
        );
        assert_uint_arg(&decoded, 4, "deadline", U256::from(9_999_999_999u64));
    }

    #[test]
    fn test_decode_swap_eth_for_exact_tokens() {
        let fixture = Fixture {
            chain_id: 1,
            rpc: Rpc {
                params: vec![TxParam {
                    to: UNISWAP_V2_ROUTER_MAINNET.to_owned(),
                    data: String::new(),
                }],
            },
        };
        let to = address(&fixture.rpc.params[0].to);
        let value = decimal("1500000000000000000");

        let decoded = SwapETHForExactTokensDecoder::new()
            .decode(
                &context(&fixture, &to, &value),
                &encoded_swap_eth_for_exact_tokens(),
            )
            .unwrap();

        assert_eq!(
            decoded.decoder_id,
            DecoderId::new("uniswap-v2/swapETHForExactTokens")
        );
        assert_eq!(
            decoded.function_signature,
            "swapETHForExactTokens(uint256,address[],address,uint256)"
        );
        assert!(decoded.nested.is_empty());
        assert_eq!(decoded.args.len(), 4);
        assert_uint_arg(&decoded, 0, "amountOut", U256::from(200_000_000u64));
        assert_path_arg(
            &decoded,
            1,
            &[
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                "0xdac17f958d2ee523a2206206994597c13d831ec7",
            ],
        );
        assert_address_arg(
            &decoded,
            2,
            "to",
            "0x1111111111111111111111111111111111111111",
        );
        assert_uint_arg(&decoded, 3, "deadline", U256::from(9_999_999_999u64));
    }

    #[test]
    fn test_match_keys_cover_mainnet() {
        let router = address(UNISWAP_V2_ROUTER_MAINNET);

        assert!(SwapExactTokensForTokensDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router.clone(),
                selector: SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR,
            }));
        assert!(SwapTokensForExactTokensDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router.clone(),
                selector: SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR,
            }));
        assert!(SwapExactETHForTokensDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router.clone(),
                selector: SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR,
            }));
        assert!(SwapTokensForExactETHDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router.clone(),
                selector: SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR,
            }));
        assert!(SwapExactTokensForETHDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router.clone(),
                selector: SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR,
            }));
        assert!(SwapETHForExactTokensDecoder::new()
            .match_keys()
            .contains(&crate::CallMatchKey {
                chain_id: 1,
                to: router,
                selector: SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR,
            }));
    }
}
