//! Generic Tier A [`Mapper`] implementation driven by an
//! [`AdapterFunctionBundle`].
//!
//! Spec §5.4 (DeclarativeMapper struct), §5.5 (Bridge: DecodedCall.decoder_id).
//!
//! Phase 1A wired the `single_emit` strategy. Phase 4 added `multicall_recurse`
//! (delegated to [`super::multicall::execute`]). Phase 5 adds
//! `opcode_stream_dispatch` (delegated to [`super::opcode_stream::execute`]).
//! Phase 12.0 activates `enum_tagged_dispatch` (delegated to
//! [`super::enum_tagged::execute`]) — Balancer V2 `userData` / Curve Router NG
//! per-hop swap-type tables. All four DSL strategies are now live.

use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::ActionEnvelope;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

use super::enum_tagged;
use super::multicall;
use super::opcode_stream;
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
            EmitRule::OpcodeStreamDispatch { .. } => {
                opcode_stream::execute(ctx, decoded, &self.bundle.emit)
            }
            EmitRule::EnumTaggedDispatch { .. } => {
                enum_tagged::execute(ctx, decoded, &self.bundle.emit)
            }
            EmitRule::MulticallRecurse { .. } => multicall::execute(ctx, decoded, &self.bundle.emit),
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
        Action, Address, AmountKind, AssetKind, Category, DecimalString, ValiditySource,
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
            parent_calldata: None,
            depth: 0,
            resolver: None,
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
    const V3_EXACT_OUTPUT_BUNDLE: &str =
        include_str!("../../tests/fixtures/uniswap-v3-exact-output.json");
    const V3_EXACT_OUTPUT_SINGLE_BUNDLE: &str =
        include_str!("../../tests/fixtures/uniswap-v3-exact-output-single.json");
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

    // ──────────────────────────────────────────────────────────────────────
    // Phase 9A.2 — V3 SR01 exactOutput / exactOutputSingle bundles
    //
    // V3 `exactOutput` uses a **reversed** packed path: the output token comes
    // first (offset 0..20), the input token comes last. The declarative bundle
    // therefore maps `unfold_v3_path(path, "last_token")` to `inputToken` and
    // `unfold_v3_path(path, "first_token")` to `outputToken`. `derive_swap_mode`
    // auto-detects `(Max, Exact)` → `SwapMode::ExactOut`.
    //
    // SwapRouter01 (`0xE592...1564`) returns `validity` populated from the
    // `deadline` argument, matching the static `map_exact_output*` helpers.
    // ──────────────────────────────────────────────────────────────────────

    /// V3 `exactOutput` — `params` tuple flattened to 5 args.
    fn v3_exact_output_decoded(decoder_id: DecoderId) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature: "exactOutput((bytes,address,uint256,uint256,uint256))".into(),
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
                    name: "amountOut".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_u64)),
                },
                DecodedArg {
                    name: "amountInMaximum".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(2_000_000_u64)),
                },
            ],
            nested: vec![],
        }
    }

    /// V3 `exactOutputSingle` — `params` tuple flattened to 8 args.
    fn v3_exact_output_single_decoded(decoder_id: DecoderId) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature:
                "exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))"
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
                    name: "amountOut".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(100_000_u64)),
                },
                DecodedArg {
                    name: "amountInMaximum".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(200_000_000_u64)),
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

    #[test]
    fn declarative_equivalent_to_static_v3_exact_output() {
        use crate::protocols::uniswap_v3::UniswapV3ExactOutputMapper;

        let static_decoded = v3_exact_output_decoded(DecoderId::new(
            abi_resolver::ids::EXACT_OUTPUT_DECODER_ID,
        ));
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let static_env = UniswapV3ExactOutputMapper::new()
            .map(&ctx, &static_decoded)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let declarative_env = run_declarative(V3_EXACT_OUTPUT_BUNDLE, v3_exact_output_decoded);

        let Action::Swap(s) = &static_env.action else {
            panic!("static is not swap");
        };
        let Action::Swap(d) = &declarative_env.action else {
            panic!("declarative is not swap");
        };
        // ExactOut swap_mode is derived from (Max, Exact) amount-kind pairing.
        assert_eq!(s.swap_mode, policy_engine::action::dex::SwapMode::ExactOut);
        // Path's first hop fee = 500 → first_fee/100 = 5.
        assert_swap_equivalent(s, d, true, Some(5));

        // Reversed path: `last_token` = input, `first_token` = output.
        assert_eq!(d.input_token.asset.address, Some(v3_last_token()));
        assert_eq!(d.output_token.asset.address, Some(v3_first_token()));
    }

    #[test]
    fn declarative_equivalent_to_static_v3_exact_output_single() {
        use crate::protocols::uniswap_v3::UniswapV3ExactOutputSingleMapper;

        let static_decoded = v3_exact_output_single_decoded(DecoderId::new(
            abi_resolver::ids::EXACT_OUTPUT_SINGLE_DECODER_ID,
        ));
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let static_env = UniswapV3ExactOutputSingleMapper::new()
            .map(&ctx, &static_decoded)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let declarative_env =
            run_declarative(V3_EXACT_OUTPUT_SINGLE_BUNDLE, v3_exact_output_single_decoded);

        let Action::Swap(s) = &static_env.action else {
            panic!("static is not swap");
        };
        let Action::Swap(d) = &declarative_env.action else {
            panic!("declarative is not swap");
        };
        assert_eq!(s.swap_mode, policy_engine::action::dex::SwapMode::ExactOut);
        // fee = 3000 → fee/100 = 30.
        assert_swap_equivalent(s, d, true, Some(30));
        // Single hop — tokenIn / tokenOut explicit (path semantics not used).
        assert_eq!(d.input_token.asset.address, Some(v3_first_token()));
        assert_eq!(d.output_token.asset.address, Some(v3_last_token()));
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 12.3 — Curve Router NG `exchange` (single_emit + curve_route_last_token)
    //
    // End-to-end coverage: bundle JSON → declarative_decoder_id → DecodedCall
    // with a 3-hop `_route` → envelope. Verifies (a) `select_address` resolves
    // idx 0 to the input token, (b) `curve_route_last_token` resolves the
    // output token by picking the last non-zero even-index entry (idx 6 in a
    // 3-hop route — idx 8/10 are zero-padded).
    // ──────────────────────────────────────────────────────────────────────

    const CURVE_ROUTER_NG_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-router-ng-exchange.json");

    /// USDC mainnet — input token of the synthetic 3-hop route.
    fn curve_input_token() -> Address {
        Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap()
    }

    /// WBTC mainnet — output token of the synthetic 3-hop route (idx 6).
    fn curve_output_token() -> Address {
        Address::from_str("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599").unwrap()
    }

    /// Receiver — distinct from `recipient()` so the test asserts the bundle
    /// uses `$.args._receiver`, not `$.tx.from` or the V2 path.
    fn curve_receiver() -> Address {
        Address::from_str("0x5555555555555555555555555555555555555555").unwrap()
    }

    /// Zero address — used to fill unused route / pool slots. Materialized
    /// per-call because `policy_engine::action::Address` does not impl `Copy`.
    fn curve_zero_addr() -> Address {
        Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
    }

    /// 3-hop route: USDC -> USDT -> DAI -> WBTC. Indices 8 / 10 are
    /// zero-padded (Router NG always emits an 11-slot array).
    fn curve_router_ng_three_hop_decoded(decoder_id: DecoderId) -> DecodedCall {
        // Pool placeholders (odd indices) — concrete values don't matter for
        // the resolver, only the non-zero-ness of even indices does. We use
        // distinct dummies for clarity.
        let pool_1 = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let pool_2 = Address::from_str("0x2222222222222222222222222222222222222222").unwrap();
        let pool_3 = Address::from_str("0x3333333333333333333333333333333333333333").unwrap();
        let usdt = Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap();
        let dai = Address::from_str("0x6b175474e89094c44da98b954eedeac495271d0f").unwrap();

        let route_addrs: Vec<DecodedValue> = vec![
            DecodedValue::Address(curve_input_token()),   // [0] USDC
            DecodedValue::Address(pool_1),                // [1] pool 1
            DecodedValue::Address(usdt),                  // [2] USDT
            DecodedValue::Address(pool_2),                // [3] pool 2
            DecodedValue::Address(dai),                   // [4] DAI
            DecodedValue::Address(pool_3),                // [5] pool 3
            DecodedValue::Address(curve_output_token()),  // [6] WBTC
            DecodedValue::Address(curve_zero_addr()),     // [7] padded
            DecodedValue::Address(curve_zero_addr()),     // [8] padded
            DecodedValue::Address(curve_zero_addr()),     // [9] padded
            DecodedValue::Address(curve_zero_addr()),     // [10] padded
        ];

        // _swap_params is a 5x5 of uint256. All zero — the bundle does not
        // read this field (Router NG inner swap_type is forward-spec).
        let zero_uint = DecodedValue::Uint(U256::ZERO);
        let inner_row = DecodedValue::Array(vec![zero_uint; 5]);
        let swap_params = DecodedValue::Array(vec![inner_row; 5]);

        // _pools (address[5]) — all zero, also unread by the bundle.
        let pools = DecodedValue::Array(
            (0..5)
                .map(|_| DecodedValue::Address(curve_zero_addr()))
                .collect(),
        );

        DecodedCall {
            decoder_id,
            function_signature:
                "exchange(address[11],uint256[5][5],uint256,uint256,address[5],address)".into(),
            args: vec![
                DecodedArg {
                    name: "_route".into(),
                    abi_type: "address[11]".into(),
                    value: DecodedValue::Array(route_addrs),
                },
                DecodedArg {
                    name: "_swap_params".into(),
                    abi_type: "uint256[5][5]".into(),
                    value: swap_params,
                },
                DecodedArg {
                    name: "_amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_u64)), // 1 USDC (6 dp)
                },
                DecodedArg {
                    name: "_expected".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(2_100_u64)), // 2100 satoshi
                },
                DecodedArg {
                    name: "_pools".into(),
                    abi_type: "address[5]".into(),
                    value: pools,
                },
                DecodedArg {
                    name: "_receiver".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(curve_receiver()),
                },
            ],
            nested: vec![],
        }
    }

    #[test]
    fn declarative_curve_router_ng_exchange_three_hop() {
        let envelope =
            run_declarative(CURVE_ROUTER_NG_BUNDLE, curve_router_ng_three_hop_decoded);

        assert_eq!(envelope.category, Category::Dex);

        let Action::Swap(action) = &envelope.action else {
            panic!("expected Swap action, got {:?}", envelope.action);
        };

        // Curve Router NG semantics — exact-in, min-out, no on-chain deadline
        // field (the bundle omits validity entirely).
        assert_eq!(action.swap_mode, policy_engine::action::dex::SwapMode::ExactIn);

        // Input = first non-zero address (idx 0) — USDC.
        assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.input_token.asset.address, Some(curve_input_token()));
        assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.input_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000".to_owned())
        );

        // Output = curve_route_last_token(_route) — last non-zero even idx (6) = WBTC.
        assert_eq!(action.output_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.output_token.asset.address, Some(curve_output_token()));
        assert_eq!(action.output_token.amount.kind, AmountKind::Min);
        assert_eq!(
            action.output_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("2100".to_owned())
        );

        // Recipient = $.args._receiver — distinct from $.tx.from.
        assert_eq!(action.recipient, curve_receiver());

        // The bundle does not emit fee_bps or validity for Router NG.
        assert!(action.fee_bps.is_none());
        assert!(action.validity.is_none());
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 12.4 — Curve Stableswap V1/NG liquidity ops + V1 exchange variants
    //
    // End-to-end coverage of the Phase 12.2 bundle set (3pool / steth / NG)
    // exercising:
    //   * `add_liquidity` → `AddLiquidityAction` (N=3 inputs via `read_assets_array`)
    //   * `remove_liquidity` proportional → `RemoveLiquidityAction` with
    //     `exit_mode = Proportional` and 3 outputs
    //   * `remove_liquidity_one_coin` → `exit_mode = SingleAsset`, **PoC hardcodes
    //     i=0 (DAI)** — other coin indices fall back to the static mapper
    //   * `remove_liquidity_imbalance` (Phase 12.4 new bundle) →
    //     `exit_mode = ExactOut`, outputs have `kind = Exact`, lp has `kind = Max`
    //   * V1 exchange variants (3pool DAI→USDC; steth native ETH→stETH)
    //   * NG exchange with `_receiver` (recipient ≠ `$.tx.from`)
    //
    // The bundles hardcode `(i,j)` pair selection for V1 swap and `i` for
    // `remove_liquidity_one_coin`. The test fixtures match the hardcoded
    // values — see PoC limitation note in the bundle comments / CLAUDE.md.
    // ──────────────────────────────────────────────────────────────────────

    const CURVE_3POOL_EXCHANGE_V1_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-3pool-exchange-v1.json");
    const CURVE_3POOL_ADD_LIQUIDITY_3_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-3pool-add-liquidity-3.json");
    const CURVE_3POOL_REMOVE_LIQUIDITY_3_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-3pool-remove-liquidity-3.json");
    const CURVE_3POOL_REMOVE_LIQUIDITY_ONE_COIN_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-3pool-remove-liquidity-one-coin.json");
    const CURVE_3POOL_REMOVE_LIQUIDITY_IMBALANCE_3_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-3pool-remove-liquidity-imbalance-3.json");
    const CURVE_STETH_EXCHANGE_V1_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-steth-exchange-v1.json");
    const CURVE_CRVUSD_USDC_EXCHANGE_NG_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-crvusd-usdc-exchange-ng.json");

    // ── Address constants (Curve mainnet) ──────────────────────────────────
    /// DAI mainnet — 3pool index 0.
    fn curve_3pool_dai() -> Address {
        Address::from_str("0x6b175474e89094c44da98b954eedeac495271d0f").unwrap()
    }
    /// USDC mainnet — 3pool index 1.
    fn curve_3pool_usdc() -> Address {
        Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap()
    }
    /// USDT mainnet — 3pool index 2.
    fn curve_3pool_usdt() -> Address {
        Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap()
    }
    /// 3CRV LP token (`0x6c3f...e490`).
    fn curve_3pool_lp() -> Address {
        Address::from_str("0x6c3f90f043a72fa612cbac8115ee7e52bde6e490").unwrap()
    }
    /// 3pool address (also the LP token contract on V1 pools — `0xbebc...c7`).
    fn curve_3pool_addr() -> Address {
        Address::from_str("0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7").unwrap()
    }
    /// stETH mainnet (`0xae7a...e84`).
    fn curve_steth() -> Address {
        Address::from_str("0xae7ab96520de3a18e5e111b5eaab095312d7fe84").unwrap()
    }
    /// crvUSD mainnet (`0xf939...b4e`).
    fn curve_crvusd() -> Address {
        Address::from_str("0xf939e0a03fb07f59a73314e73794be0e57ac1b4e").unwrap()
    }

    /// Helper to build a Curve V1 `exchange(int128 i, int128 j, uint256 dx, uint256 min_dy)`
    /// DecodedCall. `i` / `j` are passed as `int128` via `DecodedValue::Int`.
    /// The bundle does **not** read these args (it hardcodes input/output
    /// addresses for the PoC) — they are included for ABI shape correctness.
    fn curve_v1_exchange_decoded(decoder_id: DecoderId, i: i64, j: i64, dx: u64, min_dy: u64)
        -> DecodedCall
    {
        DecodedCall {
            decoder_id,
            function_signature: "exchange(int128,int128,uint256,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "i".into(),
                    abi_type: "int128".into(),
                    value: DecodedValue::Int(alloy_primitives::I256::try_from(i).unwrap()),
                },
                DecodedArg {
                    name: "j".into(),
                    abi_type: "int128".into(),
                    value: DecodedValue::Int(alloy_primitives::I256::try_from(j).unwrap()),
                },
                DecodedArg {
                    name: "dx".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(dx)),
                },
                DecodedArg {
                    name: "min_dy".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(min_dy)),
                },
            ],
            nested: vec![],
        }
    }

    #[test]
    fn declarative_curve_3pool_exchange_v1_dai_to_usdc() {
        // 3pool `exchange` — PoC hardcodes i=0 (DAI in), j=1 (USDC out).
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_3POOL_EXCHANGE_V1_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = curve_v1_exchange_decoded(
            mapper.declarative_decoder_id(),
            0, // i = DAI
            1, // j = USDC
            1_000_000_000_000_000_000_u64, // 1e18 DAI (18 dp)
            900_000_u64,                   // 0.9 USDC (6 dp min_dy)
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Dex);
        let Action::Swap(action) = &envelope.action else {
            panic!("expected Swap action, got {:?}", envelope.action);
        };

        assert_eq!(action.swap_mode, SwapMode::ExactIn);
        // Input = DAI (hardcoded literal in bundle).
        assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.input_token.asset.address, Some(curve_3pool_dai()));
        assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.input_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000".to_owned())
        );
        // Output = USDC (hardcoded literal in bundle).
        assert_eq!(action.output_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.output_token.asset.address, Some(curve_3pool_usdc()));
        assert_eq!(action.output_token.amount.kind, AmountKind::Min);
        assert_eq!(
            action.output_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("900000".to_owned())
        );
        // Recipient = $.tx.from (V1 pools have no `_receiver` arg).
        assert_eq!(action.recipient, from);
        // V1 exchange bundle omits validity and fee_bps.
        assert!(action.validity.is_none());
        assert!(action.fee_bps.is_none());
    }

    #[test]
    fn declarative_curve_3pool_add_liquidity_3() {
        // `add_liquidity(uint256[3] amounts, uint256 min_mint_amount)`
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_3POOL_ADD_LIQUIDITY_3_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let amounts = DecodedValue::Array(vec![
            DecodedValue::Uint(U256::from(1_000_000_000_000_000_000_u64)), // 1e18 DAI
            DecodedValue::Uint(U256::from(1_000_000_u64)),                 // 1 USDC (6 dp)
            DecodedValue::Uint(U256::from(1_000_000_u64)),                 // 1 USDT (6 dp)
        ]);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "add_liquidity(uint256[3],uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "amounts".into(),
                    abi_type: "uint256[3]".into(),
                    value: amounts,
                },
                DecodedArg {
                    name: "min_mint_amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_u64)),
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Dex);
        let Action::AddLiquidity(action) = &envelope.action else {
            panic!("expected AddLiquidity action, got {:?}", envelope.action);
        };

        // Pool = 3pool contract.
        assert_eq!(action.pool.address, curve_3pool_addr());
        // N=3 input tokens — verifies `read_assets_array` generalizes beyond N=2.
        assert_eq!(action.inputs.len(), 3);
        // input[0] = DAI / Max / 1e18
        assert_eq!(action.inputs[0].asset.kind, AssetKind::Erc20);
        assert_eq!(action.inputs[0].asset.address, Some(curve_3pool_dai()));
        assert_eq!(action.inputs[0].amount.kind, AmountKind::Max);
        assert_eq!(
            action.inputs[0].amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000".to_owned())
        );
        // input[1] = USDC / Max / 1e6
        assert_eq!(action.inputs[1].asset.address, Some(curve_3pool_usdc()));
        assert_eq!(action.inputs[1].amount.kind, AmountKind::Max);
        assert_eq!(
            action.inputs[1].amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000".to_owned())
        );
        // input[2] = USDT / Max / 1e6
        assert_eq!(action.inputs[2].asset.address, Some(curve_3pool_usdt()));
        assert_eq!(action.inputs[2].amount.kind, AmountKind::Max);
        // Output LP = 3CRV / Min / 1
        assert_eq!(action.output_lp.asset.address, Some(curve_3pool_lp()));
        assert_eq!(action.output_lp.amount.kind, AmountKind::Min);
        assert_eq!(
            action.output_lp.amount.value.as_ref().map(|v| v.to_string()),
            Some("1".to_owned())
        );
        // recipient = $.tx.from
        assert_eq!(action.recipient, from);
        // V1 add_liquidity bundle omits validity.
        assert!(action.validity.is_none());
    }

    #[test]
    fn declarative_curve_3pool_remove_liquidity_3() {
        // `remove_liquidity(uint256 _amount, uint256[3] _min_amounts)`
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_3POOL_REMOVE_LIQUIDITY_3_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let min_amounts = DecodedValue::Array(vec![
            DecodedValue::Uint(U256::from(1_u64)),
            DecodedValue::Uint(U256::from(1_u64)),
            DecodedValue::Uint(U256::from(1_u64)),
        ]);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "remove_liquidity(uint256,uint256[3])".into(),
            args: vec![
                DecodedArg {
                    name: "_amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_000_000_000_000_u64)), // 1e18 LP
                },
                DecodedArg {
                    name: "_min_amounts".into(),
                    abi_type: "uint256[3]".into(),
                    value: min_amounts,
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Dex);
        let Action::RemoveLiquidity(action) = &envelope.action else {
            panic!("expected RemoveLiquidity action, got {:?}", envelope.action);
        };

        assert_eq!(
            action.exit_mode,
            policy_engine::action::dex::RemoveLiquidityExitMode::Proportional
        );
        assert_eq!(action.pool.address, curve_3pool_addr());
        // inputLp = 3CRV / Exact / 1e18
        assert_eq!(action.input_lp.asset.address, Some(curve_3pool_lp()));
        assert_eq!(action.input_lp.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.input_lp.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000".to_owned())
        );
        // N=3 outputs.
        assert_eq!(action.outputs.len(), 3);
        assert_eq!(action.outputs[0].asset.address, Some(curve_3pool_dai()));
        assert_eq!(action.outputs[0].amount.kind, AmountKind::Min);
        assert_eq!(action.outputs[1].asset.address, Some(curve_3pool_usdc()));
        assert_eq!(action.outputs[2].asset.address, Some(curve_3pool_usdt()));
        assert_eq!(action.recipient, from);
        assert!(action.validity.is_none());
    }

    #[test]
    fn declarative_curve_3pool_remove_liquidity_one_coin() {
        // `remove_liquidity_one_coin(uint256 _token_amount, int128 i, uint256 _min_amount)`
        // PoC limit: bundle hardcodes i=0 (DAI). Other i values fall through to
        // the static mapper.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_3POOL_REMOVE_LIQUIDITY_ONE_COIN_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "remove_liquidity_one_coin(uint256,int128,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "_token_amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_000_000_000_000_u64)), // 1e18 LP
                },
                DecodedArg {
                    name: "i".into(),
                    abi_type: "int128".into(),
                    value: DecodedValue::Int(alloy_primitives::I256::try_from(0_i64).unwrap()),
                },
                DecodedArg {
                    name: "_min_amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_u64)),
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Dex);
        let Action::RemoveLiquidity(action) = &envelope.action else {
            panic!("expected RemoveLiquidity action, got {:?}", envelope.action);
        };
        assert_eq!(
            action.exit_mode,
            policy_engine::action::dex::RemoveLiquidityExitMode::SingleAsset
        );
        // inputLp = 3CRV / Exact / 1e18
        assert_eq!(action.input_lp.asset.address, Some(curve_3pool_lp()));
        assert_eq!(action.input_lp.amount.kind, AmountKind::Exact);
        // Single output = DAI / Min / 1 (PoC i=0 hardcoded).
        assert_eq!(action.outputs.len(), 1);
        assert_eq!(action.outputs[0].asset.address, Some(curve_3pool_dai()));
        assert_eq!(action.outputs[0].amount.kind, AmountKind::Min);
        assert_eq!(
            action.outputs[0].amount.value.as_ref().map(|v| v.to_string()),
            Some("1".to_owned())
        );
        assert_eq!(action.recipient, from);
    }

    #[test]
    fn declarative_curve_3pool_remove_liquidity_imbalance() {
        // `remove_liquidity_imbalance(uint256[3] amounts, uint256 max_burn_amount)`
        // Bundle emits exit_mode = exact_out — outputs.amount.kind = Exact,
        // inputLp.amount.kind = Max (burner caps the LP burned).
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_3POOL_REMOVE_LIQUIDITY_IMBALANCE_3_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let amounts = DecodedValue::Array(vec![
            DecodedValue::Uint(U256::from(500_000_000_000_000_000_u64)), // 0.5 DAI
            DecodedValue::Uint(U256::from(500_000_u64)),                 // 0.5 USDC
            DecodedValue::Uint(U256::from(500_000_u64)),                 // 0.5 USDT
        ]);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "remove_liquidity_imbalance(uint256[3],uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "amounts".into(),
                    abi_type: "uint256[3]".into(),
                    value: amounts,
                },
                DecodedArg {
                    name: "max_burn_amount".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(2_000_000_000_000_000_000_u64)), // 2e18 LP cap
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Dex);
        let Action::RemoveLiquidity(action) = &envelope.action else {
            panic!("expected RemoveLiquidity action, got {:?}", envelope.action);
        };
        assert_eq!(
            action.exit_mode,
            policy_engine::action::dex::RemoveLiquidityExitMode::ExactOut
        );
        assert_eq!(action.pool.address, curve_3pool_addr());
        // inputLp.amount.kind = Max (the bundle caps lp_burn).
        assert_eq!(action.input_lp.asset.address, Some(curve_3pool_lp()));
        assert_eq!(action.input_lp.amount.kind, AmountKind::Max);
        assert_eq!(
            action.input_lp.amount.value.as_ref().map(|v| v.to_string()),
            Some("2000000000000000000".to_owned())
        );
        // N=3 outputs with Exact amounts.
        assert_eq!(action.outputs.len(), 3);
        assert_eq!(action.outputs[0].asset.address, Some(curve_3pool_dai()));
        assert_eq!(action.outputs[0].amount.kind, AmountKind::Exact);
        assert_eq!(
            action.outputs[0].amount.value.as_ref().map(|v| v.to_string()),
            Some("500000000000000000".to_owned())
        );
        assert_eq!(action.outputs[1].asset.address, Some(curve_3pool_usdc()));
        assert_eq!(action.outputs[1].amount.kind, AmountKind::Exact);
        assert_eq!(action.outputs[2].asset.address, Some(curve_3pool_usdt()));
        assert_eq!(action.outputs[2].amount.kind, AmountKind::Exact);
        assert_eq!(action.recipient, from);
    }

    /// Curve ETH placeholder address. Used by stETH / frxETH V1 pools to
    /// represent native ETH in their `coins[]` arrays.
    fn curve_eth_placeholder() -> Address {
        Address::from_str("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap()
    }

    #[test]
    fn declarative_curve_steth_exchange_v1_eth_to_steth() {
        // stETH/ETH pool — i=0 (ETH placeholder in), j=1 (stETH out).
        // Post-P0-2 bundle: coins[] = [eth_placeholder, stETH] and the
        // input/output address is now resolved via `select_from_literal_array`.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_STETH_EXCHANGE_V1_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = curve_v1_exchange_decoded(
            mapper.declarative_decoder_id(),
            0, // i = ETH
            1, // j = stETH
            1_000_000_000_000_000_000_u64, // 1 ETH
            900_000_000_000_000_000_u64,   // 0.9 stETH
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Dex);
        let Action::Swap(action) = &envelope.action else {
            panic!("expected Swap action, got {:?}", envelope.action);
        };
        // Input = ETH placeholder (coins[0]).
        assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
        assert_eq!(
            action.input_token.asset.address,
            Some(curve_eth_placeholder())
        );
        assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.input_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000".to_owned())
        );
        // Output = stETH (coins[1]).
        assert_eq!(action.output_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.output_token.asset.address, Some(curve_steth()));
        assert_eq!(action.output_token.amount.kind, AmountKind::Min);
        assert_eq!(
            action.output_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("900000000000000000".to_owned())
        );
        // Recipient = $.tx.from (V1 pools have no `_receiver` arg).
        assert_eq!(action.recipient, from);
        assert!(action.validity.is_none());
        assert!(action.fee_bps.is_none());
    }

    #[test]
    fn declarative_curve_3pool_exchange_v1_usdt_to_dai() {
        // P0-2 regression — (i=2, j=0) maps USDT → DAI. The old bundle
        // hardcoded `coins[0]` / `coins[1]`, so this tx silently mislabelled
        // input as DAI and output as USDC. The new `select_from_literal_array`
        // resolves the addresses per-args.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_3POOL_EXCHANGE_V1_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = curve_v1_exchange_decoded(
            mapper.declarative_decoder_id(),
            2, // i = USDT
            0, // j = DAI
            1_000_000_u64, // 1 USDT (6 dp)
            900_000_000_000_000_000_u64, // 0.9 DAI (18 dp)
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();
        let Action::Swap(action) = &envelope.action else {
            panic!("expected Swap action, got {:?}", envelope.action);
        };
        // Input = USDT (coins[2]).
        assert_eq!(action.input_token.asset.address, Some(curve_3pool_usdt()));
        assert_eq!(
            action.input_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000".to_owned())
        );
        // Output = DAI (coins[0]).
        assert_eq!(action.output_token.asset.address, Some(curve_3pool_dai()));
    }

    #[test]
    fn declarative_curve_steth_exchange_v1_steth_to_eth() {
        // P0-2 regression — (i=1, j=0) on stETH pool maps stETH → ETH.
        // Before the fix this tx silently mislabelled input as ETH and
        // output as stETH (i.e. inverted the swap direction).
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_STETH_EXCHANGE_V1_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = curve_v1_exchange_decoded(
            mapper.declarative_decoder_id(),
            1, // i = stETH
            0, // j = ETH
            1_000_000_000_000_000_000_u64, // 1 stETH
            900_000_000_000_000_000_u64,   // 0.9 ETH
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();
        let Action::Swap(action) = &envelope.action else {
            panic!("expected Swap action, got {:?}", envelope.action);
        };
        // Input = stETH (coins[1]).
        assert_eq!(action.input_token.asset.address, Some(curve_steth()));
        // Output = ETH placeholder (coins[0]).
        assert_eq!(
            action.output_token.asset.address,
            Some(curve_eth_placeholder())
        );
    }

    #[test]
    fn declarative_curve_crvusd_usdc_exchange_ng() {
        // NG pool `exchange(int128,int128,uint256 _dx,uint256 _min_dy,address _receiver)`
        // Recipient resolves to `$.args._receiver`, distinct from `$.tx.from`.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_CRVUSD_USDC_EXCHANGE_NG_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "exchange(int128,int128,uint256,uint256,address)".into(),
            args: vec![
                DecodedArg {
                    name: "i".into(),
                    abi_type: "int128".into(),
                    value: DecodedValue::Int(alloy_primitives::I256::try_from(0_i64).unwrap()),
                },
                DecodedArg {
                    name: "j".into(),
                    abi_type: "int128".into(),
                    value: DecodedValue::Int(alloy_primitives::I256::try_from(1_i64).unwrap()),
                },
                DecodedArg {
                    name: "_dx".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_000_000_000_000_u64)), // 1e18 crvUSD
                },
                DecodedArg {
                    name: "_min_dy".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(900_000_u64)), // 0.9 USDC
                },
                DecodedArg {
                    name: "_receiver".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(curve_receiver()),
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Dex);
        let Action::Swap(action) = &envelope.action else {
            panic!("expected Swap action, got {:?}", envelope.action);
        };
        // Input = crvUSD (hardcoded literal in bundle).
        assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.input_token.asset.address, Some(curve_crvusd()));
        assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.input_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000".to_owned())
        );
        // Output = USDC.
        assert_eq!(action.output_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.output_token.asset.address, Some(curve_3pool_usdc()));
        assert_eq!(action.output_token.amount.kind, AmountKind::Min);
        assert_eq!(
            action.output_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("900000".to_owned())
        );
        // Recipient = $.args._receiver, NOT $.tx.from.
        assert_eq!(action.recipient, curve_receiver());
        assert!(action.validity.is_none());
        assert!(action.fee_bps.is_none());
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 12.5 — crvUSD Controller (LLAMMA) bundles
    //
    // Verify that the declarative path produces a `Borrow` envelope for
    // `create_loan(uint256,uint256,uint256)` and a `Liquidate` envelope for
    // `liquidate(address,uint256)` on the wstETH controller. The bundle
    // hardcodes the controller's collateral / debt addresses (per-controller),
    // and the borrowed amount flows from `$.args.debt`.
    // ──────────────────────────────────────────────────────────────────────

    const CURVE_CRVUSD_WSTETH_CREATE_LOAN_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-crvusd-wsteth-create-loan.json");
    const CURVE_CRVUSD_WSTETH_LIQUIDATE_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-crvusd-wsteth-liquidate.json");

    /// wstETH crvUSD Controller mainnet address (`0x100d...c6ce`).
    fn curve_crvusd_wsteth_controller() -> Address {
        Address::from_str("0x100daa78fc509db39ef7d04de0c1abd299f4c6ce").unwrap()
    }

    /// wstETH mainnet (`0x7f39...2ca0`).
    fn curve_wsteth() -> Address {
        Address::from_str("0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0").unwrap()
    }

    #[test]
    fn declarative_curve_crvusd_wsteth_create_loan() {
        // create_loan(uint256 collateral, uint256 debt, uint256 N) — Borrow.
        // The PoC bundle ignores the collateral / N args and emits only the
        // debt-mint side, since the collateral deposit reaches the position
        // owner via the same call's internal token-pull.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_CRVUSD_WSTETH_CREATE_LOAN_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "create_loan(uint256,uint256,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "collateral".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(2_000_000_000_000_000_000_u64)), // 2 wstETH
                },
                DecodedArg {
                    name: "debt".into(),
                    abi_type: "uint256".into(),
                    // 5000 crvUSD (18 dp)
                    value: DecodedValue::Uint(
                        U256::from(5_000_u64) * U256::from(1_000_000_000_000_000_000_u64),
                    ),
                },
                DecodedArg {
                    name: "N".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(10_u64)),
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Lending);
        let Action::Borrow(action) = &envelope.action else {
            panic!("expected Borrow action, got {:?}", envelope.action);
        };
        // Borrowed asset = crvUSD.
        assert_eq!(action.asset.kind, AssetKind::Erc20);
        assert_eq!(action.asset.address, Some(curve_crvusd()));
        assert_eq!(action.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.amount.value.as_ref().map(|v| v.to_string()),
            Some("5000000000000000000000".to_owned())
        );
        // Market = wstETH controller.
        let market = action.market.as_ref().expect("market present");
        assert_eq!(market.address.as_ref(), Some(&curve_crvusd_wsteth_controller()));
        assert_eq!(market.label.as_deref(), Some("Curve crvUSD wstETH Controller"));
        // Recipient = onBehalf = $.tx.from.
        assert_eq!(action.recipient, from);
        assert_eq!(action.on_behalf, from);
        assert!(action.validity.is_none());
    }

    #[test]
    fn declarative_curve_crvusd_wsteth_liquidate() {
        // liquidate(address user, uint256 min_x) — Liquidate.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_CRVUSD_WSTETH_LIQUIDATE_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let borrower = dummy_addr(0xCC);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "liquidate(address,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "user".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(borrower.clone()),
                },
                DecodedArg {
                    name: "min_x".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_000_000_000_000_000_u64)),
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Lending);
        let Action::Liquidate(action) = &envelope.action else {
            panic!("expected Liquidate action, got {:?}", envelope.action);
        };
        assert_eq!(action.borrower, borrower);
        let collateral = action.collateral_asset.as_ref().expect("collateral asset");
        assert_eq!(collateral.address.as_ref(), Some(&curve_wsteth()));
        assert_eq!(action.debt_asset.address.as_ref(), Some(&curve_crvusd()));
        let market = action.market.as_ref().expect("market present");
        assert_eq!(market.address.as_ref(), Some(&curve_crvusd_wsteth_controller()));
        // P1-1 — Curve `liquidate`'s `min_x` is the minimum *debt asset*
        // (crvUSD) the liquidator receives, not collateral seized. It maps to
        // `debtToCover` (kind=min); `seizedCollateralAmount` stays unset.
        let min_debt = action
            .debt_to_cover
            .as_ref()
            .expect("debtToCover present");
        assert_eq!(min_debt.kind, AmountKind::Min);
        assert_eq!(
            min_debt.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000".to_owned())
        );
        assert!(action.seized_collateral_amount.is_none());
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 12.6 — veCRV / Gauge / GaugeController bundles
    // ──────────────────────────────────────────────────────────────────────

    const CURVE_VECRV_CREATE_LOCK_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-vecrv-create-lock.json");
    const CURVE_GAUGE_3POOL_CLAIM_REWARDS_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-gauge-3pool-claim-rewards.json");
    const CURVE_GAUGE_CONTROLLER_VOTE_BUNDLE: &str =
        include_str!("../../tests/fixtures/curve-gauge-controller-vote.json");

    /// CRV mainnet (`0xD533...cd52`).
    fn curve_crv_token() -> Address {
        Address::from_str("0xd533a949740bb3306d119cc777fa900ba034cd52").unwrap()
    }

    /// veCRV mainnet (`0x5f3b...e2a2`).
    fn curve_vecrv() -> Address {
        Address::from_str("0x5f3b5dfeb7b28cdbd7faba78963ee202a494e2a2").unwrap()
    }

    /// 3pool Gauge mainnet (`0xbFcF...952A`).
    fn curve_3pool_gauge() -> Address {
        Address::from_str("0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a").unwrap()
    }

    #[test]
    fn declarative_curve_vecrv_create_lock() {
        // create_lock(uint256 _value, uint256 _unlock_time) — Stake.
        // The bundle ignores `_unlock_time` (no validity field in the schema).
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_VECRV_CREATE_LOCK_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "create_lock(uint256,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "_value".into(),
                    abi_type: "uint256".into(),
                    // 1000 CRV.
                    value: DecodedValue::Uint(
                        U256::from(1_000_u64) * U256::from(1_000_000_000_000_000_000_u64),
                    ),
                },
                DecodedArg {
                    name: "_unlock_time".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_900_000_000_u64)),
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::LiquidStaking);
        let Action::Stake(action) = &envelope.action else {
            panic!("expected Stake action, got {:?}", envelope.action);
        };
        assert_eq!(action.token_in.kind, AssetKind::Erc20);
        assert_eq!(action.token_in.address, Some(curve_crv_token()));
        assert_eq!(action.receipt_token.address, Some(curve_vecrv()));
        assert_eq!(action.amount_in.kind, AmountKind::Exact);
        assert_eq!(
            action.amount_in.value.as_ref().map(|v| v.to_string()),
            Some("1000000000000000000000".to_owned())
        );
        assert_eq!(action.recipient, from);
        assert!(action.amount_out.is_none());
    }

    #[test]
    fn declarative_curve_gauge_3pool_claim_rewards() {
        // claim_rewards() — no args, recipient and from resolved from
        // $.tx.from. Source records the gauge address.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_GAUGE_3POOL_CLAIM_REWARDS_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "claim_rewards()".into(),
            args: vec![],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Misc);
        let Action::ClaimRewards(action) = &envelope.action else {
            panic!("expected ClaimRewards action, got {:?}", envelope.action);
        };
        let source = action.source.as_ref().expect("source present");
        assert_eq!(source.address.as_ref(), Some(&curve_3pool_gauge()));
        assert_eq!(source.label.as_deref(), Some("Curve 3pool Gauge"));
        assert_eq!(action.from, from);
        assert_eq!(action.recipient, from);
        assert!(action.reward_tokens.is_none());
    }

    #[test]
    fn declarative_curve_gauge_controller_vote() {
        // vote_for_gauge_weights(address _gauge_addr, uint256 _user_weight) —
        // Vote. The bundle maps `_gauge_addr` to `governance` and
        // `_user_weight` to `votingPower`, with `support = "for"` constant.
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(CURVE_GAUGE_CONTROLLER_VOTE_BUNDLE).unwrap();
        let mapper = DeclarativeMapper::new(bundle);
        let decoded = DecodedCall {
            decoder_id: mapper.declarative_decoder_id(),
            function_signature: "vote_for_gauge_weights(address,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "_gauge_addr".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(curve_3pool_gauge()),
                },
                DecodedArg {
                    name: "_user_weight".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(10_000_u64)), // 100% in basis points
                },
            ],
            nested: vec![],
        };

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);
        let envelope = mapper.map(&ctx, &decoded).unwrap().into_iter().next().unwrap();

        assert_eq!(envelope.category, Category::Misc);
        let Action::Vote(action) = &envelope.action else {
            panic!("expected Vote action, got {:?}", envelope.action);
        };
        assert_eq!(action.governance, curve_3pool_gauge());
        assert_eq!(action.governance_label.as_deref(), Some("Curve GaugeController"));
        assert_eq!(action.proposal_id.to_string(), "0");
        assert!(matches!(
            action.support,
            policy_engine::action::misc::VoteSupport::For
        ));
        assert_eq!(
            action.voting_power.as_ref().map(|v| v.to_string()),
            Some("10000".to_owned())
        );
        assert!(action.validity.is_none());
    }
}
