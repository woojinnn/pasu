use std::collections::HashMap;
use std::str::FromStr as _;

use abi_resolver::ids::{
    EXACT_OUTPUT_DECODER_ID, EXACT_OUTPUT_SINGLE_DECODER_ID, UNISWAP_V3_DECODER_ID,
};
use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity, ValiditySource,
};
use serde::Deserialize;

use super::exact_input_single::{UniswapV3Mapper, UNISWAP_V3_MAPPER_ID};
use super::exact_output::UniswapV3ExactOutputMapper;
use super::exact_output_single::UniswapV3ExactOutputSingleMapper;
use crate::{EmptyTokenRegistry, MapContext, Mapper as _, TokenMetadata, TokenRegistry};

struct StaticTokenRegistry {
    tokens: HashMap<(u64, Address), TokenMetadata>,
}

impl StaticTokenRegistry {
    fn new(tokens: impl IntoIterator<Item = (u64, Address, TokenMetadata)>) -> Self {
        Self {
            tokens: tokens
                .into_iter()
                .map(|(chain_id, address, metadata)| ((chain_id, address), metadata))
                .collect(),
        }
    }
}

impl TokenRegistry for StaticTokenRegistry {
    fn lookup(&self, chain_id: u64, address: &Address) -> Option<TokenMetadata> {
        self.tokens.get(&(chain_id, address.clone())).cloned()
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Fixture {
    chain_id: u64,
    rpc: Rpc,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Rpc {
    params: Vec<TxParam>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct TxParam {
    from: String,
    to: String,
    data: String,
}

fn address(value: &str) -> Address {
    Address::from_str(value).unwrap()
}

fn decimal(value: &str) -> DecimalString {
    DecimalString::from_str(value).unwrap()
}

fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
    AmountConstraint {
        kind,
        value: Some(decimal(value)),
    }
}

fn erc20(chain_id: u64, address: &str, symbol: Option<&str>, decimals: Option<u8>) -> AssetRef {
    AssetRef {
        kind: AssetKind::Erc20,
        chain_id,
        address: Some(address.parse().unwrap()),
        symbol: symbol.map(str::to_owned),
        decimals,
    }
}

fn metadata_registry() -> StaticTokenRegistry {
    StaticTokenRegistry::new([
        (
            1,
            address("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            TokenMetadata {
                symbol: "USDT".to_owned(),
                decimals: 6,
            },
        ),
        (
            1,
            address("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
            TokenMetadata {
                symbol: "USDC".to_owned(),
                decimals: 6,
            },
        ),
        (
            1,
            address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
            TokenMetadata {
                symbol: "WETH".to_owned(),
                decimals: 18,
            },
        ),
    ])
}

fn ctx<'a>(
    token_registry: &'a dyn TokenRegistry,
    from: &'a Address,
    to: &'a Address,
    value_wei: &'a DecimalString,
) -> MapContext<'a> {
    MapContext {
        chain_id: 1,
        from,
        to,
        value_wei,
        block_timestamp: Some(1_700_000_000),
        token_registry,
    }
}

fn exact_input_single_decoded() -> DecodedCall {
    DecodedCall {
        decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
        function_signature:
            "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))"
                .to_owned(),
        args: vec![
            DecodedArg {
                name: "tokenIn".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0xdac17f958d2ee523a2206206994597c13d831ec7",
                )),
            },
            DecodedArg {
                name: "tokenOut".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                )),
            },
            DecodedArg {
                name: "fee".to_owned(),
                abi_type: "uint24".to_owned(),
                value: DecodedValue::Uint(U256::from(3000u64)),
            },
            DecodedArg {
                name: "recipient".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0x1111111111111111111111111111111111111111",
                )),
            },
            DecodedArg {
                name: "deadline".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(9_999_999_999u64)),
            },
            DecodedArg {
                name: "amountIn".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(200_000_000u64)),
            },
            DecodedArg {
                name: "amountOutMinimum".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::ZERO),
            },
            DecodedArg {
                name: "sqrtPriceLimitX96".to_owned(),
                abi_type: "uint160".to_owned(),
                value: DecodedValue::Uint(U256::ZERO),
            },
        ],
        nested: vec![],
    }
}

fn exact_input_decoded() -> DecodedCall {
    DecodedCall {
        decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
        function_signature: "exactInput((bytes,address,uint256,uint256,uint256))".to_owned(),
        args: vec![
            DecodedArg {
                name: "path".to_owned(),
                abi_type: "bytes".to_owned(),
                value: DecodedValue::Bytes(
                    hex::decode(
                        "dac17f958d2ee523a2206206994597c13d831ec70001f4a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000bb8c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    )
                    .unwrap(),
                ),
            },
            DecodedArg {
                name: "recipient".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0x1111111111111111111111111111111111111111",
                )),
            },
            DecodedArg {
                name: "deadline".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::ONE),
            },
            DecodedArg {
                name: "amountIn".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(1_000_000u64)),
            },
            DecodedArg {
                name: "amountOutMinimum".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::ZERO),
            },
        ],
        nested: vec![],
    }
}

fn exact_output_single_decoded() -> DecodedCall {
    DecodedCall {
        decoder_id: DecoderId::new(EXACT_OUTPUT_SINGLE_DECODER_ID),
        function_signature:
            "exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))"
                .to_owned(),
        args: vec![
            DecodedArg {
                name: "tokenIn".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0xdac17f958d2ee523a2206206994597c13d831ec7",
                )),
            },
            DecodedArg {
                name: "tokenOut".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                )),
            },
            DecodedArg {
                name: "fee".to_owned(),
                abi_type: "uint24".to_owned(),
                value: DecodedValue::Uint(U256::from(3000u64)),
            },
            DecodedArg {
                name: "recipient".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0x1111111111111111111111111111111111111111",
                )),
            },
            DecodedArg {
                name: "deadline".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(9_999_999_999u64)),
            },
            DecodedArg {
                name: "amountOut".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(100_000_000u64)),
            },
            DecodedArg {
                name: "amountInMaximum".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(200_000_000u64)),
            },
            DecodedArg {
                name: "sqrtPriceLimitX96".to_owned(),
                abi_type: "uint160".to_owned(),
                value: DecodedValue::Uint(U256::ZERO),
            },
        ],
        nested: vec![],
    }
}

fn exact_output_decoded() -> DecodedCall {
    DecodedCall {
        decoder_id: DecoderId::new(EXACT_OUTPUT_DECODER_ID),
        function_signature: "exactOutput((bytes,address,uint256,uint256,uint256))".to_owned(),
        args: vec![
            DecodedArg {
                name: "path".to_owned(),
                abi_type: "bytes".to_owned(),
                value: DecodedValue::Bytes(
                    hex::decode(
                        "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000bb8dac17f958d2ee523a2206206994597c13d831ec7",
                    )
                    .unwrap(),
                ),
            },
            DecodedArg {
                name: "recipient".to_owned(),
                abi_type: "address".to_owned(),
                value: DecodedValue::Address(address(
                    "0x1111111111111111111111111111111111111111",
                )),
            },
            DecodedArg {
                name: "deadline".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::ONE),
            },
            DecodedArg {
                name: "amountOut".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(1_000_000u64)),
            },
            DecodedArg {
                name: "amountInMaximum".to_owned(),
                abi_type: "uint256".to_owned(),
                value: DecodedValue::Uint(U256::from(2_000_000u64)),
            },
        ],
        nested: vec![],
    }
}

fn expected_exact_input_single_envelope(symbols: bool) -> ActionEnvelope {
    let (in_symbol, in_decimals, out_symbol, out_decimals) = if symbols {
        (Some("USDT"), Some(6), Some("WETH"), Some(18))
    } else {
        (None, None, None, None)
    };
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(SwapAction {
            mode: SwapMode::ExactIn,
            token_in: erc20(
                1,
                "0xdac17f958d2ee523a2206206994597c13d831ec7",
                in_symbol,
                in_decimals,
            ),
            token_out: erc20(
                1,
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                out_symbol,
                out_decimals,
            ),
            amount_in: amount(AmountKind::Exact, "200000000"),
            amount_out: amount(AmountKind::Min, "0"),
            recipient: address("0x1111111111111111111111111111111111111111"),
            validity: Some(Validity {
                expires_at: decimal("9999999999"),
                source: ValiditySource::TxDeadline,
            }),
            fee_bps: Some(30),
            enrichment: SwapEnrichment::default(),
        }),
    }
}

fn expected_exact_input_envelope(symbols: bool) -> ActionEnvelope {
    let (in_symbol, in_decimals, out_symbol, out_decimals) = if symbols {
        (Some("USDT"), Some(6), Some("WETH"), Some(18))
    } else {
        (None, None, None, None)
    };
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(SwapAction {
            mode: SwapMode::ExactIn,
            token_in: erc20(
                1,
                "0xdac17f958d2ee523a2206206994597c13d831ec7",
                in_symbol,
                in_decimals,
            ),
            token_out: erc20(
                1,
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                out_symbol,
                out_decimals,
            ),
            amount_in: amount(AmountKind::Exact, "1000000"),
            amount_out: amount(AmountKind::Min, "0"),
            recipient: address("0x1111111111111111111111111111111111111111"),
            validity: Some(Validity {
                expires_at: decimal("1"),
                source: ValiditySource::TxDeadline,
            }),
            fee_bps: Some(5),
            enrichment: SwapEnrichment::default(),
        }),
    }
}

#[test]
fn test_map_exact_input_single_produces_swap_action() {
    let token_registry = EmptyTokenRegistry;
    let from = address("0x0000000000000000000000000000000000000001");
    let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
    let value_wei = decimal("0");

    let result = UniswapV3Mapper::new()
        .map(
            &ctx(&token_registry, &from, &to, &value_wei),
            &exact_input_single_decoded(),
        )
        .unwrap();

    assert_eq!(UniswapV3Mapper::new().id().as_str(), UNISWAP_V3_MAPPER_ID);
    assert_eq!(result, vec![expected_exact_input_single_envelope(false)]);
    let Action::Swap(swap) = &result[0].action else {
        panic!("expected swap action");
    };
    assert_eq!(result[0].category, Category::Dex);
    assert_eq!(swap.mode, SwapMode::ExactIn);
    assert_eq!(swap.amount_in.kind, AmountKind::Exact);
    assert_eq!(swap.amount_out.kind, AmountKind::Min);
    assert_eq!(swap.fee_bps, Some(30));
}

#[test]
fn test_map_exact_input_parses_path_and_uses_first_hop_fee() {
    let token_registry = EmptyTokenRegistry;
    let from = address("0x0000000000000000000000000000000000000001");
    let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
    let value_wei = decimal("0");

    let result = UniswapV3Mapper::new()
        .map(
            &ctx(&token_registry, &from, &to, &value_wei),
            &exact_input_decoded(),
        )
        .unwrap();

    assert_eq!(result, vec![expected_exact_input_envelope(false)]);
    let Action::Swap(swap) = &result[0].action else {
        panic!("expected swap action");
    };
    assert_eq!(
        swap.token_in.address,
        Some(address("0xdac17f958d2ee523a2206206994597c13d831ec7"))
    );
    assert_eq!(
        swap.token_out.address,
        Some(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
    );
    assert_eq!(swap.fee_bps, Some(5));
}

#[test]
fn maps_exact_output_single_to_swap_action() {
    let token_registry = EmptyTokenRegistry;
    let from = address("0x0000000000000000000000000000000000000001");
    let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
    let value_wei = decimal("0");

    let result = UniswapV3ExactOutputSingleMapper::new()
        .map(
            &ctx(&token_registry, &from, &to, &value_wei),
            &exact_output_single_decoded(),
        )
        .unwrap();

    assert_eq!(result.len(), 1);
    let Action::Swap(swap) = &result[0].action else {
        panic!("expected swap action");
    };
    assert_eq!(result[0].category, Category::Dex);
    assert_eq!(swap.mode, SwapMode::ExactOut);
    assert_eq!(swap.amount_in, amount(AmountKind::Max, "200000000"));
    assert_eq!(swap.amount_out, amount(AmountKind::Exact, "100000000"));
    assert_eq!(
        swap.token_in.address,
        Some(address("0xdac17f958d2ee523a2206206994597c13d831ec7"))
    );
    assert_eq!(
        swap.token_out.address,
        Some(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
    );
    assert_eq!(
        swap.recipient,
        address("0x1111111111111111111111111111111111111111")
    );
    assert_eq!(
        swap.validity,
        Some(Validity {
            expires_at: decimal("9999999999"),
            source: ValiditySource::TxDeadline,
        })
    );
    assert_eq!(swap.fee_bps, Some(30));
    assert_eq!(swap.enrichment, SwapEnrichment::default());
}

#[test]
fn maps_exact_output_path_uses_reverse_direction() {
    let token_registry = EmptyTokenRegistry;
    let from = address("0x0000000000000000000000000000000000000001");
    let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
    let value_wei = decimal("0");

    let result = UniswapV3ExactOutputMapper::new()
        .map(
            &ctx(&token_registry, &from, &to, &value_wei),
            &exact_output_decoded(),
        )
        .unwrap();

    assert_eq!(result.len(), 1);
    let Action::Swap(swap) = &result[0].action else {
        panic!("expected swap action");
    };
    assert_eq!(swap.mode, SwapMode::ExactOut);
    assert_eq!(swap.amount_in.kind, AmountKind::Max);
    assert_eq!(swap.amount_out.kind, AmountKind::Exact);
    assert_eq!(
        swap.token_in.address,
        Some(address("0xdac17f958d2ee523a2206206994597c13d831ec7"))
    );
    assert_eq!(
        swap.token_out.address,
        Some(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
    );
    assert_eq!(swap.fee_bps, Some(30));
}

#[test]
fn test_map_uses_token_registry_for_symbol() {
    let token_registry = metadata_registry();
    let from = address("0x0000000000000000000000000000000000000001");
    let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
    let value_wei = decimal("0");

    let result = UniswapV3Mapper::new()
        .map(
            &ctx(&token_registry, &from, &to, &value_wei),
            &exact_input_decoded(),
        )
        .unwrap();

    assert_eq!(result, vec![expected_exact_input_envelope(true)]);
    let Action::Swap(swap) = &result[0].action else {
        panic!("expected swap action");
    };
    assert_eq!(swap.token_in.symbol.as_deref(), Some("USDT"));
    assert_eq!(swap.token_in.decimals, Some(6));
    assert_eq!(swap.token_out.symbol.as_deref(), Some("WETH"));
    assert_eq!(swap.token_out.decimals, Some(18));
}
