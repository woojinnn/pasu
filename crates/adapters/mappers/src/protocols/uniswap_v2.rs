use std::str::FromStr as _;

use abi_resolver::{DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity, ValiditySource,
};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_MAPPER_ID: &str = "uniswap-v2/swapExactTokensForTokens";
pub const SWAP_TOKENS_FOR_EXACT_TOKENS_MAPPER_ID: &str = "uniswap-v2/swapTokensForExactTokens";

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapExactTokensForTokensMapper;

impl SwapExactTokensForTokensMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for SwapExactTokensForTokensMapper {
    fn id(&self) -> MapperId {
        MapperId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_MAPPER_ID)
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let amount_in = uint_arg(decoded, "amountIn")?;
        let amount_out_min = uint_arg(decoded, "amountOutMin")?;
        let path = address_array_arg(decoded, "path")?;
        let recipient = address_arg(decoded, "to")?;
        let deadline = uint_arg(decoded, "deadline")?;
        let (token_in, token_out) = path_assets(ctx, &path)?;

        Ok(vec![swap_envelope(SwapAction {
            mode: SwapMode::ExactIn,
            token_in,
            token_out,
            amount_in: amount_constraint(AmountKind::Exact, amount_in)?,
            amount_out: amount_constraint(AmountKind::Min, amount_out_min)?,
            recipient,
            slippage_bps: None,
            validity: Some(validity(deadline)?),
            fee_bps: Some(30),
            enrichment: SwapEnrichment::default(),
        })])
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapTokensForExactTokensMapper;

impl SwapTokensForExactTokensMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for SwapTokensForExactTokensMapper {
    fn id(&self) -> MapperId {
        MapperId::new(SWAP_TOKENS_FOR_EXACT_TOKENS_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(SWAP_TOKENS_FOR_EXACT_TOKENS_MAPPER_ID)
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let amount_out = uint_arg(decoded, "amountOut")?;
        let amount_in_max = uint_arg(decoded, "amountInMax")?;
        let path = address_array_arg(decoded, "path")?;
        let recipient = address_arg(decoded, "to")?;
        let deadline = uint_arg(decoded, "deadline")?;
        let (token_in, token_out) = path_assets(ctx, &path)?;

        Ok(vec![swap_envelope(SwapAction {
            mode: SwapMode::ExactOut,
            token_in,
            token_out,
            amount_in: amount_constraint(AmountKind::Max, amount_in_max)?,
            amount_out: amount_constraint(AmountKind::Exact, amount_out)?,
            recipient,
            slippage_bps: None,
            validity: Some(validity(deadline)?),
            fee_bps: Some(30),
            enrichment: SwapEnrichment::default(),
        })])
    }
}

fn swap_envelope(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    }
}

fn path_assets(
    ctx: &MapContext<'_>,
    path: &[Address],
) -> Result<(AssetRef, AssetRef), MapperError> {
    if path.len() < 2 {
        return Err(MapperError::ArgumentMismatch {
            name: "path".to_owned(),
            message: format!("expected at least two token addresses, got {}", path.len()),
        });
    }
    let token_in = path.first().expect("path length checked");
    let token_out = path.last().expect("path length checked");

    Ok((asset_ref(ctx, token_in), asset_ref(ctx, token_out)))
}

fn asset_ref(ctx: &MapContext<'_>, address: &Address) -> AssetRef {
    let metadata = ctx.token_registry.lookup(ctx.chain_id, address);
    AssetRef {
        kind: AssetKind::Erc20,
        chain_id: ctx.chain_id,
        address: Some(address.clone()),
        symbol: metadata.as_ref().map(|m| m.symbol.clone()),
        decimals: metadata.map(|m| m.decimals),
    }
}

fn amount_constraint(kind: AmountKind, value: U256) -> Result<AmountConstraint, MapperError> {
    Ok(AmountConstraint {
        kind,
        value: Some(decimal(value)?),
    })
}

fn validity(deadline: U256) -> Result<Validity, MapperError> {
    Ok(Validity {
        expires_at: decimal(deadline)?,
        source: ValiditySource::TxDeadline,
    })
}

fn decimal(value: U256) -> Result<DecimalString, MapperError> {
    DecimalString::from_str(&value.to_string())
        .map_err(|e| MapperError::Internal(anyhow::anyhow!(e)))
}

fn uint_arg(decoded: &DecodedCall, name: &str) -> Result<U256, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Uint(value) => Ok(*value),
        other => Err(argument_mismatch(
            name,
            format!("expected uint256, got {other:?}"),
        )),
    }
}

fn address_arg(decoded: &DecodedCall, name: &str) -> Result<Address, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Address(value) => Ok(value.clone()),
        other => Err(argument_mismatch(
            name,
            format!("expected address, got {other:?}"),
        )),
    }
}

fn address_array_arg(decoded: &DecodedCall, name: &str) -> Result<Vec<Address>, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Array(values) => values
            .iter()
            .map(|value| match value {
                DecodedValue::Address(address) => Ok(address.clone()),
                other => Err(argument_mismatch(
                    name,
                    format!("expected address[] item, got {other:?}"),
                )),
            })
            .collect(),
        other => Err(argument_mismatch(
            name,
            format!("expected address[], got {other:?}"),
        )),
    }
}

fn arg<'a>(decoded: &'a DecodedCall, name: &str) -> Result<&'a DecodedValue, MapperError> {
    decoded
        .args
        .iter()
        .find(|arg| arg.name == name)
        .map(|arg| &arg.value)
        .ok_or_else(|| MapperError::MissingArgument(name.to_owned()))
}

fn argument_mismatch(name: &str, message: String) -> MapperError {
    MapperError::ArgumentMismatch {
        name: name.to_owned(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::{SwapExactTokensForTokensMapper, SwapTokensForExactTokensMapper};
    use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
    use alloy_primitives::U256;
    use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use policy_engine::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        Category, DecimalString, Validity, ValiditySource,
    };
    use std::collections::HashMap;
    use std::str::FromStr as _;

    use crate::{EmptyTokenRegistry, MapContext, Mapper as _, TokenMetadata, TokenRegistry};

    const EXACT_IN_DECODER_ID: &str = "uniswap-v2/swapExactTokensForTokens";
    const EXACT_OUT_DECODER_ID: &str = "uniswap-v2/swapTokensForExactTokens";

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

    fn exact_in_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(EXACT_IN_DECODER_ID),
            function_signature:
                "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)".to_owned(),
            args: vec![
                DecodedArg {
                    name: "amountIn".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(200_000_000u64)),
                },
                DecodedArg {
                    name: "amountOutMin".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::ZERO),
                },
                DecodedArg {
                    name: "path".to_owned(),
                    abi_type: "address[]".to_owned(),
                    value: DecodedValue::Array(vec![
                        DecodedValue::Address(address(
                            "0xdac17f958d2ee523a2206206994597c13d831ec7",
                        )),
                        DecodedValue::Address(address(
                            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        )),
                    ]),
                },
                DecodedArg {
                    name: "to".to_owned(),
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
            ],
            nested: vec![],
        }
    }

    fn exact_out_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(EXACT_OUT_DECODER_ID),
            function_signature:
                "swapTokensForExactTokens(uint256,uint256,address[],address,uint256)".to_owned(),
            args: vec![
                DecodedArg {
                    name: "amountOut".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(1_000_000_000_000_000_000u64)),
                },
                DecodedArg {
                    name: "amountInMax".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(4_000_000_000u64)),
                },
                DecodedArg {
                    name: "path".to_owned(),
                    abi_type: "address[]".to_owned(),
                    value: DecodedValue::Array(vec![
                        DecodedValue::Address(address(
                            "0xdac17f958d2ee523a2206206994597c13d831ec7",
                        )),
                        DecodedValue::Address(address(
                            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        )),
                    ]),
                },
                DecodedArg {
                    name: "to".to_owned(),
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
            ],
            nested: vec![],
        }
    }

    fn expected_exact_in_envelope(symbols: bool) -> ActionEnvelope {
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
                slippage_bps: None,
                validity: Some(Validity {
                    expires_at: decimal("9999999999"),
                    source: ValiditySource::TxDeadline,
                }),
                fee_bps: Some(30),
                enrichment: SwapEnrichment::default(),
            }),
        }
    }

    fn expected_exact_out_envelope(symbols: bool) -> ActionEnvelope {
        let (in_symbol, in_decimals, out_symbol, out_decimals) = if symbols {
            (Some("USDT"), Some(6), Some("WETH"), Some(18))
        } else {
            (None, None, None, None)
        };
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(SwapAction {
                mode: SwapMode::ExactOut,
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
                amount_in: amount(AmountKind::Max, "4000000000"),
                amount_out: amount(AmountKind::Exact, "1000000000000000000"),
                recipient: address("0x1111111111111111111111111111111111111111"),
                slippage_bps: None,
                validity: Some(Validity {
                    expires_at: decimal("9999999999"),
                    source: ValiditySource::TxDeadline,
                }),
                fee_bps: Some(30),
                enrichment: SwapEnrichment::default(),
            }),
        }
    }

    #[test]
    fn test_map_swap_exact_in_produces_swap_action() {
        let token_registry = EmptyTokenRegistry;
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0x7a250d5630b4cf539739df2c5dacb4c659f2488d");
        let value_wei = decimal("0");

        let result = SwapExactTokensForTokensMapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_in_decoded(),
            )
            .unwrap();

        assert_eq!(result, vec![expected_exact_in_envelope(false)]);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(result[0].category, Category::Dex);
        assert_eq!(swap.mode, SwapMode::ExactIn);
        assert_eq!(swap.amount_in.kind, AmountKind::Exact);
        assert_eq!(swap.amount_out.kind, AmountKind::Min);
    }

    #[test]
    fn test_map_swap_exact_out_produces_swap_action() {
        let token_registry = EmptyTokenRegistry;
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0x7a250d5630b4cf539739df2c5dacb4c659f2488d");
        let value_wei = decimal("0");

        let result = SwapTokensForExactTokensMapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_out_decoded(),
            )
            .unwrap();

        assert_eq!(result, vec![expected_exact_out_envelope(false)]);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(swap.mode, SwapMode::ExactOut);
        assert_eq!(swap.amount_in.kind, AmountKind::Max);
        assert_eq!(swap.amount_out.kind, AmountKind::Exact);
    }

    #[test]
    fn test_map_uses_token_registry_for_symbol() {
        let token_registry = metadata_registry();
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0x7a250d5630b4cf539739df2c5dacb4c659f2488d");
        let value_wei = decimal("0");

        let result = SwapExactTokensForTokensMapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_in_decoded(),
            )
            .unwrap();

        assert_eq!(result, vec![expected_exact_in_envelope(true)]);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(swap.token_in.symbol.as_deref(), Some("USDT"));
        assert_eq!(swap.token_in.decimals, Some(6));
        assert_eq!(swap.token_out.symbol.as_deref(), Some("WETH"));
        assert_eq!(swap.token_out.decimals, Some(18));
    }
}
