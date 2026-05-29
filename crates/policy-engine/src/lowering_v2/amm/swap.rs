//! `Amm::Swap` lowering → `Amm::SwapContext`.
//!
//! Reference implementation for the new-model lowering pattern: build the
//! context `Map` with camelCase keys, inline `LiveField<T>` as its inner `T`,
//! omit absent optionals, and assemble via [`LowerCtx::lowered`] with the
//! namespaced action uid.

use serde_json::{Map, Value};

use simulation_reducer::action::amm::{SwapAction, SwapDirection};
use simulation_state::primitives::U256;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_amm_venue;

/// Lower an `Amm::Swap` action into the `Amm::SwapContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
// Swap happens to be infallible, but the `Result` is the shared per-action
// contract — other actions return `Err` (e.g. unsupported sub-variants).
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(swap: &SwapAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    // `route` is `LiveField<SwapRoute>`; the policy layer sees only the summed
    // estimated-out across all paths (route shape is too complex for Cedar).
    let route_estimated_out = swap
        .live_inputs
        .route
        .value
        .paths
        .iter()
        .fold(U256::ZERO, |acc, p| acc.saturating_add(p.estimated_out));

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_amm_venue(&swap.venue));
    m.insert("tokenIn".into(), lower_token_ref(&swap.params.token_in));
    m.insert("tokenOut".into(), lower_token_ref(&swap.params.token_out));
    m.insert(
        "direction".into(),
        lower_swap_direction(&swap.params.direction),
    );
    m.insert(
        "recipient".into(),
        Value::String(addr(&swap.params.recipient)),
    );
    m.insert(
        "slippageBp".into(),
        Value::from(i64::from(swap.params.slippage_bp)),
    );
    m.insert(
        "routeEstimatedOut".into(),
        Value::String(u256_hex(route_estimated_out)),
    );
    m.insert(
        "expectedAmountOut".into(),
        Value::String(u256_hex(swap.live_inputs.expected_amount_out.value)),
    );
    m.insert(
        "priceImpactBp".into(),
        Value::from(i64::from(swap.live_inputs.price_impact_bp.value)),
    );
    m.insert(
        "gasEstimate".into(),
        Value::String(u256_hex(swap.live_inputs.gas_estimate.value)),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Amm::Action::"Swap""#, Value::Object(m)))
}

/// Lower a [`SwapDirection`] → discriminated `{ kind, … }` (`Amm::SwapDirection`).
fn lower_swap_direction(direction: &SwapDirection) -> Value {
    let mut m = Map::new();
    match direction {
        SwapDirection::ExactInput {
            amount_in,
            min_amount_out,
        } => {
            m.insert("kind".into(), Value::String("exact_input".into()));
            m.insert("amountIn".into(), Value::String(u256_hex(*amount_in)));
            m.insert(
                "minAmountOut".into(),
                Value::String(u256_hex(*min_amount_out)),
            );
        }
        SwapDirection::ExactOutput {
            max_amount_in,
            amount_out,
        } => {
            m.insert("kind".into(), Value::String("exact_output".into()));
            m.insert(
                "maxAmountIn".into(),
                Value::String(u256_hex(*max_amount_in)),
            );
            m.insert("amountOut".into(), Value::String(u256_hex(*amount_out)));
        }
    }
    Value::Object(m)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::redundant_clone
)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::amm::{
        AmmAction, AmmVenue, PoolState, RouteHop, RoutePath, SwapAction, SwapDirection,
        SwapLiveInputs, SwapParams, SwapRoute,
    };
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Duration, Time, U128, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::{LiveField, NonceKey};

    use crate::lowering_v2::{lower_action, TxMeta};

    const FROM: &str = "0x1111111111111111111111111111111111111111";
    const TO: &str = "0x2222222222222222222222222222222222222222";

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// A UniswapV3 single-hop ExactInput USDC→WETH swap on Arbitrum
    /// (OnchainTx), parameterized by slippage bp.
    fn sample_swap_action(slippage_bp: u32) -> (ActionBody, ActionMeta) {
        let chain = ChainId::arbitrum();
        let usdc = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xaf88d065e77c8cc2239327c5edb3a432268e5831").unwrap(),
            },
        };
        let weth = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0x82af49447d8a07e3bd95bd0d56f35241523fbab1").unwrap(),
            },
        };
        let pool = Address::from_str("0xc6962004f452be9203591991d15f6b388e09e8d0").unwrap();

        let v3 = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool,
            fee_tier_bp: 500,
        };

        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };

        let pool_source = DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "slot0()".into(),
            decoder_id: "uniswap_v3_slot0".into(),
        };

        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10000,
                hops: vec![RouteHop {
                    token_in: usdc.clone(),
                    token_out: weth.clone(),
                    venue: v3.clone(),
                    pool_state,
                    effective_fee_bp: 5,
                    estimated_out: U256::from(305_000_000_000_000_000u64),
                }],
                estimated_out: U256::from(305_000_000_000_000_000u64),
            }],
            aggregator: None,
        };

        let swap = AmmAction::Swap(SwapAction {
            venue: v3,
            params: SwapParams {
                token_in: usdc,
                token_out: weth,
                direction: SwapDirection::ExactInput {
                    amount_in: U256::from(1_000_000_000u64),
                    min_amount_out: U256::from(300_000_000_000_000_000u64),
                },
                recipient: user(),
                slippage_bp,
            },
            live_inputs: SwapLiveInputs {
                route: LiveField::new(route, pool_source.clone(), now())
                    .with_ttl(Duration::from_secs(12)),
                expected_amount_out: LiveField::new(
                    U256::from(305_000_000_000_000_000u64),
                    pool_source.clone(),
                    now(),
                ),
                price_impact_bp: LiveField::new(12u32, pool_source, now()),
                gas_estimate: LiveField::new(
                    U256::from(180_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "gas/arbitrum".into(),
                    },
                    now(),
                ),
            },
        });

        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain,
                nonce: 42,
                gas_limit: U256::from(200_000u64),
                gas_price: LiveField::new(
                    U256::from(100_000_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "ETH/USD".into(),
                    },
                    now(),
                ),
                value: U256::ZERO,
            },
        };

        (ActionBody::Amm(swap), meta)
    }

    /// A second, deliberately different swap to widen the conformance gate:
    /// AggregatorRoute venue (router/routeHash branch), ExactOutput direction,
    /// Native `tokenOut`, and an OffchainSig meta (exercises `lower_eip712` +
    /// `lower_nature` offchain branch + `nonceKey`).
    fn sample_swap_aggregator_offchain() -> (ActionBody, ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let usdc = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        };
        // Native tokenOut exercises the TokenKey::Native branch.
        let eth = TokenRef {
            key: TokenKey::Native {
                chain: chain.clone(),
            },
        };
        let router = Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap();

        let venue = AmmVenue::AggregatorRoute {
            chain: chain.clone(),
            router,
            route_hash: "0xabc0000000000000000000000000000000000000000000000000000000000000".into(),
        };

        let hop_venue = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };

        let src = DataSource::VenueApi {
            endpoint: "https://api.1inch.dev/swap/v6.0/1/swap".into(),
            parser_id: "oneinch_v6_route".into(),
            auth: None,
        };

        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10000,
                hops: vec![RouteHop {
                    token_in: usdc.clone(),
                    token_out: eth.clone(),
                    venue: hop_venue,
                    pool_state,
                    effective_fee_bp: 5,
                    estimated_out: U256::from(300_000_000_000_000_000u64),
                }],
                estimated_out: U256::from(300_000_000_000_000_000u64),
            }],
            aggregator: None,
        };

        let swap = AmmAction::Swap(SwapAction {
            venue,
            params: SwapParams {
                token_in: usdc,
                token_out: eth,
                direction: SwapDirection::ExactOutput {
                    max_amount_in: U256::from(1_100_000_000u64),
                    amount_out: U256::from(300_000_000_000_000_000u64),
                },
                recipient: user(),
                slippage_bp: 75,
            },
            live_inputs: SwapLiveInputs {
                route: LiveField::new(route, src.clone(), now()),
                expected_amount_out: LiveField::new(
                    U256::from(300_000_000_000_000_000u64),
                    src.clone(),
                    now(),
                ),
                price_impact_bp: LiveField::new(18u32, src.clone(), now()),
                gas_estimate: LiveField::new(U256::from(280_000u64), src, now()),
            },
        });

        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Permit2".into(),
                    version: Some("1".into()),
                    chain_id: Some(1),
                    verifying_contract: Some(
                        Address::from_str("0x000000000022d473030f116ddee9f6b43ac78ba3").unwrap(),
                    ),
                    salt: None,
                },
                deadline: Time::from_unix(1_738_001_800),
                nonce_key: Some(NonceKey::OrderHash {
                    hash: "0xabc0000000000000000000000000000000000000000000000000000000000000".into(),
                }),
            },
        };

        (ActionBody::Amm(swap), meta)
    }

    /// Synthesize the swap per-policy schema (core + amm/swap, no custom fields)
    /// via the v2 manifest path — exactly what the host composes at runtime.
    fn swap_schema_text() -> String {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": "swap-schema",
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": "swap" } } }
        }))
        .unwrap();
        crate::schema::compose_per_policy(&manifest).unwrap()
    }

    /// THE GATE: a wrong rename / missing required field / wrong type makes the
    /// strict (schema-`Some`) `Context::from_json_value` ERROR here.
    #[test]
    fn swap_lowering_conforms_to_schema() {
        let (body, meta) = sample_swap_action(50);
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();

        assert_eq!(lowered.action_uid, "Amm::Action::\"Swap\"");
        assert_eq!(lowered.principal, format!("Wallet::\"{FROM}\""));
        assert_eq!(lowered.resource, format!("Protocol::\"{TO}\""));

        let schema_text = swap_schema_text();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();

        // STRICT context construction — this is the gate.
        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .expect("lowered swap context must conform to Amm::SwapContext");
    }

    /// Widen the gate: the aggregator-route + ExactOutput + Native + OffchainSig
    /// sample must ALSO conform.
    #[test]
    fn swap_lowering_aggregator_offchain_conforms_to_schema() {
        let (body, meta) = sample_swap_aggregator_offchain();
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();

        let schema_text = swap_schema_text();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();

        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .expect("aggregator/offchain swap context must conform to Amm::SwapContext");
    }

    /// Prove the full lower → eval path: a `@severity("warn")` forbid on
    /// `slippageBp > 100` passes for slippage 50 and warns for slippage 150.
    #[test]
    fn swap_end_to_end_warn_and_pass() {
        let policy = "@id(\"swap-slippage\")\n@severity(\"warn\")\n\
            forbid(principal, action == Amm::Action::\"Swap\", resource)\n\
            when { context.slippageBp > 100 };\n";
        let schema_text = swap_schema_text();
        let engine =
            crate::policy::PolicyEngine::build_from_per_policy(&[(policy.to_owned(), schema_text)])
                .unwrap();

        // slippage 50 → Pass.
        let (body, meta) = sample_swap_action(50);
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let verdict = engine
            .evaluate(
                &lowered.principal,
                &lowered.action_uid,
                &lowered.resource,
                &serde_json::json!([]),
                &lowered.context,
            )
            .unwrap();
        assert_eq!(verdict, crate::policy::Verdict::Pass, "slippage 50 must pass");

        // slippage 150 → Warn.
        let (body, meta) = sample_swap_action(150);
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let verdict = engine
            .evaluate(
                &lowered.principal,
                &lowered.action_uid,
                &lowered.resource,
                &serde_json::json!([]),
                &lowered.context,
            )
            .unwrap();
        assert!(
            matches!(verdict, crate::policy::Verdict::Warn(_)),
            "slippage 150 must warn, got {verdict:?}"
        );
    }
}
