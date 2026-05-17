//! Generic Tier A [`Mapper`] implementation driven by an
//! [`AdapterFunctionBundle`].
//!
//! Spec §5.4 (DeclarativeMapper struct), §5.5 (Bridge: DecodedCall.decoder_id).
//!
//! Phase 1A wires only the `single_emit` strategy. Other strategies
//! (`opcode_stream_dispatch`, `enum_tagged_dispatch`, `multicall_recurse`)
//! parse via Phase 0 [`super::types`] but return
//! [`MapperError::Internal("not implemented in Phase 1A")`] when invoked.

use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::ActionEnvelope;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

use super::single_emit;
use super::types::{AdapterFunctionBundle, EmitRule};

/// `DeclarativeMapper` wraps a parsed bundle and drives the matching
/// `EmitRule` strategy at `map()` time.
///
/// The struct intentionally omits the `alloy_json_abi::Function` field
/// referenced in the spec's struct sketch (§5.4). In ScopeBall's current
/// pipeline the *decoder* (abi-resolver) already produces a `DecodedCall`
/// with typed args, so the mapper does not need the raw ABI fragment. The
/// abi fragment lives on the bundle (`bundle.abi_fragment`) and is consumed
/// by the decoder side when bundle install is wired up.
#[derive(Debug, Clone)]
pub struct DeclarativeMapper {
    bundle: AdapterFunctionBundle,
}

impl DeclarativeMapper {
    /// Construct a `DeclarativeMapper` from a parsed bundle.
    #[must_use]
    pub fn new(bundle: AdapterFunctionBundle) -> Self {
        Self { bundle }
    }

    /// Borrow the underlying bundle.
    #[must_use]
    pub fn bundle(&self) -> &AdapterFunctionBundle {
        &self.bundle
    }

    /// `declarative.<bundle.id stripped of @version>` (spec §5.4:589-595).
    ///
    /// Example: bundle id `"uniswap/v2/swapExactTokensForTokens@1.0.0"` →
    /// decoder id `"declarative.uniswap/v2/swapExactTokensForTokens"`.
    ///
    /// Stable across bundle version bumps.
    #[must_use]
    pub fn declarative_decoder_id(&self) -> DecoderId {
        let path = self
            .bundle
            .id
            .split('@')
            .next()
            .unwrap_or(&self.bundle.id);
        DecoderId::new(format!("declarative.{path}"))
    }
}

impl Mapper for DeclarativeMapper {
    fn id(&self) -> MapperId {
        // Use the full bundle id (including `@version`) as the mapper id so
        // mapper registries can distinguish two bundles publishing the same
        // declarative_decoder_id at different versions.
        MapperId::new(&self.bundle.id)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == self.declarative_decoder_id()
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        match &self.bundle.emit {
            EmitRule::SingleEmit { .. } => {
                let envelope = single_emit::execute(ctx, decoded, &self.bundle.emit)?;
                Ok(vec![envelope])
            }
            EmitRule::OpcodeStreamDispatch { .. } => Err(MapperError::Internal(
                anyhow::anyhow!("opcode_stream_dispatch not implemented in Phase 1A"),
            )),
            EmitRule::EnumTaggedDispatch { .. } => Err(MapperError::Internal(
                anyhow::anyhow!("enum_tagged_dispatch not implemented in Phase 1A"),
            )),
            EmitRule::MulticallRecurse { .. } => Err(MapperError::Internal(
                anyhow::anyhow!("multicall_recurse not implemented in Phase 1A"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_resolver::{DecodedArg, DecodedValue};
    use alloy_primitives::U256;
    use policy_engine::action::dex::SwapMode;
    use policy_engine::action::{
        Action, Address, AmountKind, AssetKind, DecimalString, ValiditySource,
    };
    use std::str::FromStr as _;

    use crate::protocols::uniswap_v2::SwapExactTokensForTokensMapper;
    use crate::token_registry::EmptyTokenRegistry;

    const V2_BUNDLE_JSON: &str =
        include_str!("../../tests/fixtures/uniswap-v2-swap-exact-tokens.json");

    fn token_in() -> Address {
        Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap()
    }

    fn token_out() -> Address {
        Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap()
    }

    fn recipient() -> Address {
        Address::from_str("0x4444444444444444444444444444444444444444").unwrap()
    }

    /// Build a DecodedCall whose decoder_id matches the declarative bundle.
    fn decoded_for_declarative(decoder_id: DecoderId) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature:
                "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "amountIn".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_000_000_000_000_u128)),
                },
                DecodedArg {
                    name: "amountOutMin".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_900_000_u64)),
                },
                DecodedArg {
                    name: "path".into(),
                    abi_type: "address[]".into(),
                    value: DecodedValue::Array(vec![
                        DecodedValue::Address(token_in()),
                        DecodedValue::Address(token_out()),
                    ]),
                },
                DecodedArg {
                    name: "to".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(recipient()),
                },
                DecodedArg {
                    name: "deadline".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_700_000_900_u64)),
                },
            ],
            nested: vec![],
        }
    }

    fn dummy_addr(label: u8) -> Address {
        Address::from_str(&format!("0x{}{}", "0".repeat(38), format!("{label:02x}"))).unwrap()
    }

    fn build_ctx<'a>(
        registry: &'a EmptyTokenRegistry,
        from: &'a Address,
        to: &'a Address,
        value: &'a DecimalString,
    ) -> MapContext<'a> {
        MapContext {
            chain_id: 1,
            from,
            to,
            value_wei: value,
            block_timestamp: Some(1_700_000_000),
            token_registry: registry,
        }
    }

    #[test]
    fn declarative_decoder_id_strips_version() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(V2_BUNDLE_JSON).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        assert_eq!(
            mapper.declarative_decoder_id(),
            DecoderId::new("declarative.uniswap/v2/swapExactTokensForTokens")
        );
    }

    #[test]
    fn accepts_only_declarative_decoder_id() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(V2_BUNDLE_JSON).unwrap();
        let mapper = DeclarativeMapper::new(bundle);

        let accept = decoded_for_declarative(mapper.declarative_decoder_id());
        assert!(mapper.accepts(&accept));

        let reject = decoded_for_declarative(DecoderId::new("static.uniswap-v2/swap"));
        assert!(!mapper.accepts(&reject));
    }

    #[test]
    fn v2_swap_fixture_maps_to_swap_envelope() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(V2_BUNDLE_JSON).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = decoded_for_declarative(mapper.declarative_decoder_id());

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = mapper.map(&ctx, &decoded).unwrap();
        assert_eq!(envelopes.len(), 1);

        let Action::Swap(action) = &envelopes[0].action else {
            panic!("expected Swap action, got {:?}", envelopes[0].action);
        };

        assert_eq!(action.swap_mode, SwapMode::ExactIn);
        assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.input_token.asset.address, Some(token_in()));
        assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.input_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000".to_owned())
        );

        assert_eq!(action.output_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.output_token.asset.address, Some(token_out()));
        assert_eq!(action.output_token.amount.kind, AmountKind::Min);
        assert_eq!(
            action
                .output_token
                .amount
                .value
                .as_ref()
                .map(|v| v.to_string()),
            Some("1900000".to_owned())
        );

        assert_eq!(action.recipient, recipient());

        let validity = action.validity.as_ref().expect("validity present");
        assert_eq!(validity.source, ValiditySource::TxDeadline);
        assert_eq!(validity.expires_at.to_string(), "1700000900");

        // Phase 1A bundle does not emit fee_bps.
        assert!(action.fee_bps.is_none());
    }

    /// The declarative mapper should agree with the static V2 mapper on every
    /// field except `fee_bps` (which the V2 bundle doesn't emit) and the
    /// `AssetRef.symbol` / `.decimals` (host enrichment, also missing in PoC).
    #[test]
    fn declarative_equivalent_to_static_v2_mapper() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(V2_BUNDLE_JSON).unwrap();
        let declarative = DeclarativeMapper::new(bundle);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let static_mapper = SwapExactTokensForTokensMapper::new();

        // The static mapper expects the *static* decoder_id, while the
        // declarative mapper expects the declarative.* decoder_id. We build
        // two parallel DecodedCalls with the same args but different ids.
        let static_decoded = DecodedCall {
            decoder_id: DecoderId::new(abi_resolver::ids::SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID),
            ..decoded_for_declarative(declarative.declarative_decoder_id())
        };
        let declarative_decoded = decoded_for_declarative(declarative.declarative_decoder_id());

        let static_envs = static_mapper.map(&ctx, &static_decoded).unwrap();
        let declarative_envs = declarative.map(&ctx, &declarative_decoded).unwrap();

        assert_eq!(static_envs.len(), declarative_envs.len());
        assert_eq!(static_envs[0].category, declarative_envs[0].category);

        let Action::Swap(s) = &static_envs[0].action else {
            panic!("static is not swap");
        };
        let Action::Swap(d) = &declarative_envs[0].action else {
            panic!("declarative is not swap");
        };

        assert_eq!(s.swap_mode, d.swap_mode);
        assert_eq!(s.input_token.asset.kind, d.input_token.asset.kind);
        assert_eq!(s.input_token.asset.address, d.input_token.asset.address);
        assert_eq!(s.input_token.amount, d.input_token.amount);

        assert_eq!(s.output_token.asset.kind, d.output_token.asset.kind);
        assert_eq!(s.output_token.asset.address, d.output_token.asset.address);
        assert_eq!(s.output_token.amount, d.output_token.amount);

        assert_eq!(s.recipient, d.recipient);
        assert_eq!(s.validity, d.validity);

        // Documented gap — declarative bundle currently omits fee_bps.
        assert_eq!(s.fee_bps, Some(30));
        assert_eq!(d.fee_bps, None);
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 3 — V3 + SR02 single_emit bundles
    //
    // Equivalence with the static V3 mappers (`UniswapV3Mapper`, the SR02
    // family). The declarative bundles intentionally omit `fee_bps` (Phase 3
    // does not implement `div(uint, u32)` over a path-derived value), and the
    // SR02 bundles omit `validity` (SR02 calldata has no `deadline` —
    // deadline lives on the outer Multicall wrapper).
    // ──────────────────────────────────────────────────────────────────────

    const V3_EXACT_INPUT_BUNDLE: &str =
        include_str!("../../tests/fixtures/uniswap-v3-exact-input.json");
    const V3_EXACT_INPUT_SINGLE_BUNDLE: &str =
        include_str!("../../tests/fixtures/uniswap-v3-exact-input-single.json");
    const SR02_EXACT_INPUT_BUNDLE: &str =
        include_str!("../../tests/fixtures/sr02-exact-input.json");
    const SR02_EXACT_INPUT_SINGLE_BUNDLE: &str =
        include_str!("../../tests/fixtures/sr02-exact-input-single.json");

    /// `[USDT][fee=500=0x0001f4][USDC][fee=3000=0x000bb8][WETH]` — two hops.
    /// `decode_v3_path` returns tokens=`[USDT, USDC, WETH]`.
    fn v3_two_hop_path() -> Vec<u8> {
        hex::decode(concat!(
            "dac17f958d2ee523a2206206994597c13d831ec7",
            "0001f4",
            "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "000bb8",
            "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        ))
        .unwrap()
    }

    fn v3_first_token() -> Address {
        Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap()
    }

    fn v3_last_token() -> Address {
        Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap()
    }

    /// Build a `DecodedCall` mirroring what `bridge.rs::flatten_tuple_arg`
    /// produces for `exactInput((bytes,address,uint256,uint256,uint256))` —
    /// the wrapping `params` tuple is flattened to top-level args.
    fn v3_exact_input_decoded(decoder_id: DecoderId) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature: "exactInput((bytes,address,uint256,uint256,uint256))".into(),
            args: vec![
                DecodedArg {
                    name: "path".into(),
                    abi_type: "bytes".into(),
                    value: DecodedValue::Bytes(v3_two_hop_path()),
                },
                DecodedArg {
                    name: "recipient".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(recipient()),
                },
                DecodedArg {
                    name: "deadline".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(9_999_999_999_u64)),
                },
                DecodedArg {
                    name: "amountIn".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_u64)),
                },
                DecodedArg {
                    name: "amountOutMinimum".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(900_000_u64)),
                },
            ],
            nested: vec![],
        }
    }

    /// V3 `exactInputSingle` — `params` tuple flattened to 8 args.
    fn v3_exact_input_single_decoded(decoder_id: DecoderId) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature:
                "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))"
                    .into(),
            args: vec![
                DecodedArg {
                    name: "tokenIn".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(v3_first_token()),
                },
                DecodedArg {
                    name: "tokenOut".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(v3_last_token()),
                },
                DecodedArg {
                    name: "fee".into(),
                    abi_type: "uint24".into(),
                    value: DecodedValue::Uint(U256::from(3000_u64)),
                },
                DecodedArg {
                    name: "recipient".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(recipient()),
                },
                DecodedArg {
                    name: "deadline".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(9_999_999_999_u64)),
                },
                DecodedArg {
                    name: "amountIn".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(200_000_000_u64)),
                },
                DecodedArg {
                    name: "amountOutMinimum".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(100_000_u64)),
                },
                DecodedArg {
                    name: "sqrtPriceLimitX96".into(),
                    abi_type: "uint160".into(),
                    value: DecodedValue::Uint(U256::ZERO),
                },
            ],
            nested: vec![],
        }
    }

    /// SR02 `exactInput` — no `deadline` parameter (lives on outer Multicall).
    fn sr02_exact_input_decoded(decoder_id: DecoderId) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature: "exactInput((bytes,address,uint256,uint256))".into(),
            args: vec![
                DecodedArg {
                    name: "path".into(),
                    abi_type: "bytes".into(),
                    value: DecodedValue::Bytes(v3_two_hop_path()),
                },
                DecodedArg {
                    name: "recipient".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(recipient()),
                },
                DecodedArg {
                    name: "amountIn".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_u64)),
                },
                DecodedArg {
                    name: "amountOutMinimum".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(900_000_u64)),
                },
            ],
            nested: vec![],
        }
    }

    /// SR02 `exactInputSingle` — no `deadline`.
    fn sr02_exact_input_single_decoded(decoder_id: DecoderId) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature:
                "exactInputSingle((address,address,uint24,address,uint256,uint256,uint160))"
                    .into(),
            args: vec![
                DecodedArg {
                    name: "tokenIn".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(v3_first_token()),
                },
                DecodedArg {
                    name: "tokenOut".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(v3_last_token()),
                },
                DecodedArg {
                    name: "fee".into(),
                    abi_type: "uint24".into(),
                    value: DecodedValue::Uint(U256::from(3000_u64)),
                },
                DecodedArg {
                    name: "recipient".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(recipient()),
                },
                DecodedArg {
                    name: "amountIn".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(200_000_000_u64)),
                },
                DecodedArg {
                    name: "amountOutMinimum".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(100_000_u64)),
                },
                DecodedArg {
                    name: "sqrtPriceLimitX96".into(),
                    abi_type: "uint160".into(),
                    value: DecodedValue::Uint(U256::ZERO),
                },
            ],
            nested: vec![],
        }
    }

    /// Compare every field the declarative bundle is expected to populate.
    /// `expect_validity` controls whether `validity` must match; SR02 bundles
    /// intentionally omit it. `expect_fee_bps` is the value the *static*
    /// mapper produces — the declarative side must be `None` (Phase 3 gap).
    fn assert_swap_equivalent(
        static_swap: &policy_engine::action::dex::SwapAction,
        declarative_swap: &policy_engine::action::dex::SwapAction,
        expect_validity: bool,
        expect_static_fee_bps: Option<u32>,
    ) {
        assert_eq!(static_swap.swap_mode, declarative_swap.swap_mode);
        assert_eq!(
            static_swap.input_token.asset.kind,
            declarative_swap.input_token.asset.kind
        );
        assert_eq!(
            static_swap.input_token.asset.address,
            declarative_swap.input_token.asset.address
        );
        assert_eq!(static_swap.input_token.amount, declarative_swap.input_token.amount);

        assert_eq!(
            static_swap.output_token.asset.kind,
            declarative_swap.output_token.asset.kind
        );
        assert_eq!(
            static_swap.output_token.asset.address,
            declarative_swap.output_token.asset.address
        );
        assert_eq!(
            static_swap.output_token.amount,
            declarative_swap.output_token.amount
        );

        assert_eq!(static_swap.recipient, declarative_swap.recipient);
        if expect_validity {
            assert_eq!(static_swap.validity, declarative_swap.validity);
        } else {
            assert!(declarative_swap.validity.is_none());
        }
        // Documented gap — declarative bundle does not currently emit fee_bps.
        assert_eq!(static_swap.fee_bps, expect_static_fee_bps);
        assert!(declarative_swap.fee_bps.is_none());
    }

    /// Run the declarative bundle and return its single emitted envelope.
    fn run_declarative(
        bundle_json: &str,
        decoded_factory: impl FnOnce(DecoderId) -> DecodedCall,
    ) -> policy_engine::action::ActionEnvelope {
        let bundle: AdapterFunctionBundle = serde_json::from_str(bundle_json).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = decoded_factory(mapper.declarative_decoder_id());

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = mapper.map(&ctx, &decoded).unwrap();
        assert_eq!(envelopes.len(), 1);
        envelopes.into_iter().next().unwrap()
    }

    #[test]
    fn declarative_equivalent_to_static_v3_exact_input() {
        use crate::protocols::uniswap_v3::UniswapV3Mapper;

        let static_decoded =
            v3_exact_input_decoded(DecoderId::new(abi_resolver::ids::UNISWAP_V3_DECODER_ID));
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let static_env = UniswapV3Mapper::new()
            .map(&ctx, &static_decoded)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let declarative_env = run_declarative(V3_EXACT_INPUT_BUNDLE, v3_exact_input_decoded);

        let Action::Swap(s) = &static_env.action else {
            panic!("static is not swap");
        };
        let Action::Swap(d) = &declarative_env.action else {
            panic!("declarative is not swap");
        };
        // Path's first hop fee = 500 → first_fee/100 = 5.
        assert_swap_equivalent(s, d, true, Some(5));

        // Endpoints derived from the packed path.
        assert_eq!(d.input_token.asset.address, Some(v3_first_token()));
        assert_eq!(d.output_token.asset.address, Some(v3_last_token()));
    }

    #[test]
    fn declarative_equivalent_to_static_v3_exact_input_single() {
        use crate::protocols::uniswap_v3::UniswapV3Mapper;

        let static_decoded = v3_exact_input_single_decoded(DecoderId::new(
            abi_resolver::ids::UNISWAP_V3_DECODER_ID,
        ));
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let static_env = UniswapV3Mapper::new()
            .map(&ctx, &static_decoded)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let declarative_env =
            run_declarative(V3_EXACT_INPUT_SINGLE_BUNDLE, v3_exact_input_single_decoded);

        let Action::Swap(s) = &static_env.action else {
            panic!("static is not swap");
        };
        let Action::Swap(d) = &declarative_env.action else {
            panic!("declarative is not swap");
        };
        // fee = 3000 → fee/100 = 30.
        assert_swap_equivalent(s, d, true, Some(30));
        assert_eq!(d.input_token.asset.address, Some(v3_first_token()));
        assert_eq!(d.output_token.asset.address, Some(v3_last_token()));
    }

    #[test]
    fn declarative_equivalent_to_static_sr02_exact_input() {
        use crate::protocols::swap_router_02::Sr02ExactInputMapper;

        let static_decoded =
            sr02_exact_input_decoded(DecoderId::new(abi_resolver::ids::SR02_EXACT_INPUT_DECODER_ID));
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let static_env = Sr02ExactInputMapper::new()
            .map(&ctx, &static_decoded)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let declarative_env =
            run_declarative(SR02_EXACT_INPUT_BUNDLE, sr02_exact_input_decoded);

        let Action::Swap(s) = &static_env.action else {
            panic!("static is not swap");
        };
        let Action::Swap(d) = &declarative_env.action else {
            panic!("declarative is not swap");
        };
        // SR02 bundle omits validity; static mapper also returns None.
        assert!(s.validity.is_none());
        // first_fee = 500 → 5.
        assert_swap_equivalent(s, d, false, Some(5));
        assert_eq!(d.input_token.asset.address, Some(v3_first_token()));
        assert_eq!(d.output_token.asset.address, Some(v3_last_token()));
    }

    #[test]
    fn declarative_equivalent_to_static_sr02_exact_input_single() {
        use crate::protocols::swap_router_02::Sr02ExactInputSingleMapper;

        let static_decoded = sr02_exact_input_single_decoded(DecoderId::new(
            abi_resolver::ids::SR02_EXACT_INPUT_SINGLE_DECODER_ID,
        ));
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let static_env = Sr02ExactInputSingleMapper::new()
            .map(&ctx, &static_decoded)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let declarative_env = run_declarative(
            SR02_EXACT_INPUT_SINGLE_BUNDLE,
            sr02_exact_input_single_decoded,
        );

        let Action::Swap(s) = &static_env.action else {
            panic!("static is not swap");
        };
        let Action::Swap(d) = &declarative_env.action else {
            panic!("declarative is not swap");
        };
        assert!(s.validity.is_none());
        // fee = 3000 → 30.
        assert_swap_equivalent(s, d, false, Some(30));
        assert_eq!(d.input_token.asset.address, Some(v3_first_token()));
        assert_eq!(d.output_token.asset.address, Some(v3_last_token()));
    }
}
