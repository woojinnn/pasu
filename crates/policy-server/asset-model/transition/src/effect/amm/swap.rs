//! `SwapAction` reducer — single-pool or routed token-for-token swap.
//! Wires up the **route walk** and **venue dispatch**. Only the V2-family
//! venues (`UniswapV2`, `SushiV2`) are implemented today; concentrated-liquidity
//! (V3 / V4), aggregator outer-level handling, and the remaining stable /
//! weighted / book pools surface as `UnsupportedProtocol` until their per-feature
//! ## Balance accounting
//! Only the *outer* `token_in` / `token_out` are debited / credited. Hops in a
//! multi-hop path move strictly *inside* the swap envelope (router escrow); the
//! user's wallet only sees the first leg's spend and the last leg's receipt.
//! This matches the user-visible accounting convention of every routed-swap
//! protocol (Uniswap routers, 1inch, 0x, Paraswap): intermediate hop tokens
//! are not credited to the user.
//! ## Slippage check
//! `ExactInput` enforces `total_out >= min_amount_out` before any state
//! mutation is recorded. `ExactOutput` simulation reverses that bound (must
//! return at least `amount_out` and consume at most `max_amount_in`); the
//! requires inverse hop math (`getAmountIn`) which we defer with the rest of
//! the V3 cases.

use policy_state::primitives::{Address, U256};
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::{AmmVenue, SwapAction, SwapDirection};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::{
    aggregator, balancer_v2, balancer_v3, curve_v1, curve_v2, maverick_v2, sushi_v2, trader_joe_lb,
    uniswap_v2, uniswap_v3, uniswap_v4,
};

impl Reducer for SwapAction {
    #[allow(clippy::too_many_lines)]
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let route = &self.live_inputs.route.value;

        // Exact-output simulation uses the user-signed `max_amount_in` spend
        // cap until venue-specific inverse quotes are available.
        let total_amount_in = match &self.params.direction {
            SwapDirection::ExactInput { amount_in, .. } => *amount_in,
            SwapDirection::ExactOutput { max_amount_in, .. } => *max_amount_in,
        };

        let mut total_out = U256::ZERO;
        for path in &route.paths {
            // Pro-rata allocation of `total_amount_in` to each parallel path.
            // The aggregator route enforces `Σ share_bp == 10_000`; we trust
            // it here since route construction is validated at parse-time
            // (action/amm.rs round-trip tests cover this).
            let path_amount_in = total_amount_in
                .checked_mul(U256::from(path.share_bp))
                .ok_or_else(|| {
                    ReducerError::Invariant("swap: path_amount_in numerator overflow".into())
                })?
                / U256::from(10_000u32);

            let mut hop_in = path_amount_in;
            for hop in &path.hops {
                hop_in = match &hop.venue {
                    AmmVenue::UniswapV2 { .. } => {
                        uniswap_v2::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    // Batch 2 — SushiV2 is a UniswapV2 fork and proxies to the
                    // same closed form (`uniswap_v2::quote_swap_hop`); kept on
                    // its own arm so per-venue divergence (Trident hybrid
                    // pools, BentoBox-routed reserves) lands without a swap.rs
                    // re-edit.
                    AmmVenue::SushiV2 { .. } => {
                        sushi_v2::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    // active-tick closed form; see `uniswap_v3` module docs).
                    AmmVenue::UniswapV3 { .. } => {
                        uniswap_v3::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    // pools with a non-zero `hooks` address may override the
                    // swap curve / fees / accounting via `beforeSwap` /
                    // `afterSwap` callbacks, which we cannot soundly
                    // simulate without a known-hook registry. We reject
                    // those upfront with a distinct protocol tag so policy
                    // / observability can target them. Hooks-free pools
                    // share the V3 active-tick closed form behind the
                    // singleton, so we delegate to `uniswap_v4::quote_swap_hop`
                    // which in turn reuses the V3 helper.
                    AmmVenue::UniswapV4 { hooks, .. } => {
                        if *hooks != Address::ZERO {
                            return Err(ReducerError::UnsupportedProtocol {
                                action: "swap".into(),
                                protocol: "uniswap_v4_with_hooks".into(),
                            });
                        }
                        uniswap_v4::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    // Batch 2 — Curve / Balancer / Trader Joe LB / Maverick
                    // wired through their per-venue helpers. Each retains a
                    // distinct error tag in its Invariant messages so policy
                    // / observability can target them individually. Per-venue
                    // simplifications (Curve V2 twocrypto, Balancer balanced-
                    // weight approximation, TJ-LB / Maverick single active-bin)
                    // are documented in the corresponding venue module docs.
                    AmmVenue::CurveV1 { .. } => {
                        curve_v1::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    AmmVenue::CurveV2 { .. } => {
                        curve_v2::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    AmmVenue::BalancerV2 { .. } => {
                        balancer_v2::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    AmmVenue::BalancerV3 { .. } => {
                        balancer_v3::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    AmmVenue::TraderJoeLB { .. } => {
                        trader_joe_lb::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    AmmVenue::MaverickV2 { .. } => {
                        maverick_v2::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    // hop variant is unusual inside a path (the aggregator
                    // *route* lives in `SwapRoute.aggregator`, not as a hop
                    // venue), so we treat it as unsupported per-hop today.
                    AmmVenue::AggregatorRoute { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "aggregator_route".into(),
                        });
                    }
                };
            }
            total_out = total_out.saturating_add(hop_in);
        }

        // Slippage floor (ExactInput) — checked *before* recording any state
        // change so a slippage breach produces a pure error with an empty
        // delta.
        if let SwapDirection::ExactInput { min_amount_out, .. } = &self.params.direction {
            if total_out < *min_amount_out {
                return Err(ReducerError::Invariant(format!(
                    "swap slippage: out {total_out} < min {min_amount_out}"
                )));
            }
        }

        let mut delta = StateDelta::new();

        // `AggregatorRoute` venue — the inner per-hop variant (rejected above
        // as `UnsupportedProtocol("aggregator_route")`) stays unsupported to
        // prevent nested-aggregator dispatch. The hook order is: executor
        // allow-list check → calldata-hash sanity check → optional Permit2
        // bundle. The Permit2 `ApprovalSet` emits *before* the balance debit /
        // credit below so the resulting `TokenChange` sequence matches the
        // on-chain order (`permit` → `swap`).
        if let AmmVenue::AggregatorRoute { .. } = &self.venue {
            if let Some(agg_meta) = &route.aggregator {
                aggregator::verify_executor(agg_meta)?;
                aggregator::verify_calldata_hash(agg_meta)?;
                if agg_meta.permit_bundled {
                    aggregator::apply_permit_bundle(state, &mut delta, ctx, agg_meta, self)?;
                }
            }
        }

        helpers::balance::debit(
            state,
            &mut delta,
            &self.params.token_in.key,
            total_amount_in,
        )?;
        helpers::balance::credit(state, &mut delta, &self.params.token_out.key, total_out)?;
        Ok(delta)
    }
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{
        AggregatorKind, AggregatorMeta, AmmVenue, PoolState, RouteHop, RoutePath, SwapDirection,
        SwapLiveInputs, SwapParams, SwapRoute,
    };
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn weth_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
            },
        }
    }

    fn dai_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0x6b175474e89094c44da98b954eedeac495271d0f").unwrap(),
            },
        }
    }

    fn make_holding(token: &TokenRef, balance: U256, symbol: &str) -> TokenHolding {
        let contract = token
            .key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key: token.key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: symbol.into(),
            decimals: 18,
            balance: Balance::fungible(balance),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn state_with_pair(in_amt: U256, out_amt: U256) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens
            .insert(usdc_ref().key, make_holding(&usdc_ref(), in_amt, "USDC"));
        s.tokens
            .insert(weth_ref().key, make_holding(&weth_ref(), out_amt, "WETH"));
        s
    }

    fn v2_venue() -> AmmVenue {
        AmmVenue::UniswapV2 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap(),
            factory: Address::from_str("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f").unwrap(),
        }
    }

    fn xy_pool(reserve_in: u128, reserve_out: u128, fee_bp: u32) -> PoolState {
        PoolState::XyConstant {
            reserve_in: U256::from(reserve_in),
            reserve_out: U256::from(reserve_out),
            fee_bp,
        }
    }

    fn single_hop_route(
        token_in: TokenRef,
        token_out: TokenRef,
        venue: AmmVenue,
        pool_state: PoolState,
    ) -> SwapRoute {
        SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10_000,
                hops: vec![RouteHop {
                    token_in,
                    token_out,
                    venue,
                    pool_state,
                    effective_fee_bp: 30,
                    estimated_out: U256::ZERO,
                }],
                estimated_out: U256::ZERO,
            }],
            aggregator: None,
        }
    }

    fn make_live_inputs(route: SwapRoute) -> SwapLiveInputs {
        SwapLiveInputs {
            route: LiveField::new(route, DataSource::UserSupplied, now()),
            expected_amount_out: LiveField::new(U256::ZERO, DataSource::UserSupplied, now()),
            price_impact_bp: LiveField::new(0u32, DataSource::UserSupplied, now()),
            gas_estimate: LiveField::new(U256::ZERO, DataSource::UserSupplied, now()),
        }
    }

    fn exact_in_swap(
        amount_in: U256,
        min_out: U256,
        token_in: TokenRef,
        token_out: TokenRef,
        venue: AmmVenue,
        pool_state: PoolState,
    ) -> SwapAction {
        let route = single_hop_route(
            token_in.clone(),
            token_out.clone(),
            venue.clone(),
            pool_state,
        );
        SwapAction {
            venue,
            params: SwapParams {
                token_in,
                token_out,
                direction: SwapDirection::ExactInput {
                    amount_in,
                    min_amount_out: min_out,
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: make_live_inputs(route),
        }
    }

    // ----------------------------------------------------------------------
    // V2 single-hop happy path
    // ----------------------------------------------------------------------

    /// Single-hop V2 swap with the canonical `getAmountOut` fixture from
    /// `uniswap_v2::tests::quote_xy_constant_canonical_formula`:
    /// `1_000 in, 10_000/10_000 reserves, 30 bp fee → 906 out`. Verifies the
    /// reducer emits exactly two `BalanceDelta`s (input debit, output credit)
    /// with the matching magnitudes.
    #[test]
    fn v2_single_hop_happy_path() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::from(800u64),
            usdc_ref(),
            weth_ref(),
            v2_venue(),
            xy_pool(10_000, 10_000, 30),
        );
        let delta = swap.apply(&state, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 2);
        let mut saw_debit = false;
        let mut saw_credit = false;
        for tc in &delta.token_changes {
            let TokenChange::BalanceDelta { key, delta: d } = tc else {
                panic!("expected BalanceDelta, got {tc:?}");
            };
            if key == &usdc_ref().key {
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "1000");
                saw_debit = true;
            } else if key == &weth_ref().key {
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "906");
                saw_credit = true;
            } else {
                panic!("unexpected key {key:?}");
            }
        }
        assert!(saw_debit && saw_credit);
    }

    // ----------------------------------------------------------------------
    // V2 single-hop slippage failure
    // ----------------------------------------------------------------------

    /// `min_amount_out` set above the actually-produced amount must surface as
    /// `Invariant("swap slippage: …")` and emit no state changes.
    #[test]
    fn v2_single_hop_slippage_breach() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::from(10_000u64), // far above the ~906 actually achievable
            usdc_ref(),
            weth_ref(),
            v2_venue(),
            xy_pool(10_000, 10_000, 30),
        );
        let err = swap.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("slippage")));
    }

    // ----------------------------------------------------------------------
    // Two-path V2 split: share_bp summed across paths
    // ----------------------------------------------------------------------

    /// Two parallel V2 paths with `share_bp = 5_000 / 5_000`. Each takes
    /// `500` of the `1_000` `amount_in`; both pools are identical
    /// `10_000/10_000` `30 bp` → each hop emits
    /// `getAmountOut(500, 10_000, 10_000, 30) = 475`
    /// (closed form: `500 * 9_970 = 4_985_000`,
    ///   numerator = `4_985_000 * 10_000 = 49_850_000_000`,
    ///   denominator = `100_000_000 + 4_985_000 = 104_985_000`,
    ///   out = `474`).
    /// Sum across paths = `474 + 474 = 948`.
    #[test]
    fn v2_split_path_sums_outputs() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let route = SwapRoute {
            paths: vec![
                RoutePath {
                    share_bp: 5_000,
                    hops: vec![RouteHop {
                        token_in: usdc_ref(),
                        token_out: weth_ref(),
                        venue: v2_venue(),
                        pool_state: xy_pool(10_000, 10_000, 30),
                        effective_fee_bp: 30,
                        estimated_out: U256::ZERO,
                    }],
                    estimated_out: U256::ZERO,
                },
                RoutePath {
                    share_bp: 5_000,
                    hops: vec![RouteHop {
                        token_in: usdc_ref(),
                        token_out: weth_ref(),
                        venue: v2_venue(),
                        pool_state: xy_pool(10_000, 10_000, 30),
                        effective_fee_bp: 30,
                        estimated_out: U256::ZERO,
                    }],
                    estimated_out: U256::ZERO,
                },
            ],
            aggregator: None,
        };
        let swap = SwapAction {
            venue: v2_venue(),
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: weth_ref(),
                direction: SwapDirection::ExactInput {
                    amount_in: U256::from(1_000u64),
                    min_amount_out: U256::from(900u64),
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: make_live_inputs(route),
        };
        let delta = swap.apply(&state, &ctx()).unwrap();
        let weth_change = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if key == &weth_ref().key && delta.is_positive() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected positive WETH delta");
        // 474 + 474 = 948
        assert_eq!(weth_change, "948");
    }

    // ----------------------------------------------------------------------
    // Multi-hop within a path (intermediate token *not* tracked)
    // ----------------------------------------------------------------------

    /// USDC → DAI → WETH chained inside a single path. Only the outer
    /// `token_in` (USDC) and outer `token_out` (WETH) appear in the delta;
    /// DAI is the router's intermediate hop currency and never lands in the
    /// wallet's accounting (matches the user-visible accounting convention of
    /// every routed-swap protocol).
    #[test]
    fn v2_multi_hop_intermediate_not_credited() {
        let mut state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        state
            .tokens
            .insert(dai_ref().key, make_holding(&dai_ref(), U256::ZERO, "DAI"));

        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10_000,
                hops: vec![
                    RouteHop {
                        token_in: usdc_ref(),
                        token_out: dai_ref(),
                        venue: v2_venue(),
                        pool_state: xy_pool(10_000, 10_000, 30),
                        effective_fee_bp: 30,
                        estimated_out: U256::ZERO,
                    },
                    RouteHop {
                        token_in: dai_ref(),
                        token_out: weth_ref(),
                        venue: v2_venue(),
                        pool_state: xy_pool(10_000, 10_000, 30),
                        effective_fee_bp: 30,
                        estimated_out: U256::ZERO,
                    },
                ],
                estimated_out: U256::ZERO,
            }],
            aggregator: None,
        };
        let swap = SwapAction {
            venue: v2_venue(),
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: weth_ref(),
                direction: SwapDirection::ExactInput {
                    amount_in: U256::from(1_000u64),
                    min_amount_out: U256::ZERO,
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: make_live_inputs(route),
        };
        let delta = swap.apply(&state, &ctx()).unwrap();

        // Outer-only accounting: exactly two BalanceDeltas — USDC debit, WETH
        // credit. Even though DAI exists in `state.tokens`, the reducer must
        // not emit any DAI delta because intermediate hop tokens never land
        // in the user's accounting.
        assert_eq!(delta.token_changes.len(), 2);
        for tc in &delta.token_changes {
            let TokenChange::BalanceDelta { key, .. } = tc else {
                panic!("expected BalanceDelta");
            };
            assert!(key == &usdc_ref().key || key == &weth_ref().key);
            assert!(key != &dai_ref().key);
        }
    }

    // ----------------------------------------------------------------------
    // ----------------------------------------------------------------------

    /// `ExactOutput` direction is currently simulated as if `max_amount_in`
    /// were the spent amount (no inverse hop math). The output uses the same
    /// caught.
    #[test]
    fn v2_exact_output_uses_max_amount_in_as_spend() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let route = single_hop_route(
            usdc_ref(),
            weth_ref(),
            v2_venue(),
            xy_pool(10_000, 10_000, 30),
        );
        let swap = SwapAction {
            venue: v2_venue(),
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: weth_ref(),
                direction: SwapDirection::ExactOutput {
                    max_amount_in: U256::from(1_000u64),
                    amount_out: U256::from(500u64),
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: make_live_inputs(route),
        };
        let delta = swap.apply(&state, &ctx()).unwrap();
        // ExactOutput skips the slippage min-out check; we still record the
        // debit of `max_amount_in` and the V2 quote on it.
        assert_eq!(delta.token_changes.len(), 2);
        let usdc_debit = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if key == &usdc_ref().key && delta.is_negative() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected USDC debit");
        assert_eq!(usdc_debit, "1000");
    }

    // ----------------------------------------------------------------------
    // Unsupported venue surfaces UnsupportedProtocol
    // ----------------------------------------------------------------------

    /// (active-tick, no fee, `zeroForOne`). A degenerate pool with **zero
    /// active liquidity** must now surface as
    /// `Invariant("uniswap_v3 … zero active liquidity")` from the V3 quote
    /// behaviour.
    #[test]
    fn v3_hop_zero_liquidity_returns_invariant() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let v3 = AmmVenue::UniswapV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: policy_state::primitives::U128::from(0u64),
            ticks: vec![],
        };
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::ZERO,
            usdc_ref(),
            weth_ref(),
            v3,
            pool_state,
        );
        let err = swap.apply(&state, &ctx()).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("zero active liquidity")),
            "unexpected err: {err:?}"
        );
    }

    /// (`sqrt_price_x96 = Q96 = 2^96`) with `liquidity = 1_000_000` swapping
    /// `1_000` in. The closed form gives `L*a/(L+a) = 1e9 / 1_001_000 = 999`.
    /// Verifies the outer-only accounting (USDC debit, WETH credit) and the
    /// magnitude matches the `uniswap_v3` unit test exactly.
    #[test]
    fn v3_single_hop_happy_path_active_tick() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let v3 = AmmVenue::UniswapV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        // Q96 = 2^96
        let q96 = U256::from(1u64) << 96;
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: q96,
            tick: 0,
            liquidity: policy_state::primitives::U128::from(1_000_000u64),
            ticks: vec![],
        };
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::from(900u64), // < 999, so slippage passes
            usdc_ref(),
            weth_ref(),
            v3,
            pool_state,
        );
        let delta = swap.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 2);
        let mut saw_debit = false;
        let mut saw_credit = false;
        for tc in &delta.token_changes {
            let TokenChange::BalanceDelta { key, delta: d } = tc else {
                panic!("expected BalanceDelta, got {tc:?}");
            };
            if key == &usdc_ref().key {
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "1000");
                saw_debit = true;
            } else if key == &weth_ref().key {
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "999");
                saw_credit = true;
            } else {
                panic!("unexpected key {key:?}");
            }
        }
        assert!(saw_debit && saw_credit);
    }

    /// closed-form's `999` output produces `Invariant("swap slippage: …")`.
    #[test]
    fn v3_single_hop_slippage_breach() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let v3 = AmmVenue::UniswapV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let q96 = U256::from(1u64) << 96;
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: q96,
            tick: 0,
            liquidity: policy_state::primitives::U128::from(1_000_000u64),
            ticks: vec![],
        };
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::from(5_000u64), // far above the ~999 closed-form output
            usdc_ref(),
            weth_ref(),
            v3,
            pool_state,
        );
        let err = swap.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("slippage")));
    }

    /// (`hooks == Address::ZERO`). The math is identical to V3: a
    /// `price = 1` pool (`sqrt_price_x96 = Q96 = 2^96`) with
    /// `liquidity = 1_000_000` swapping `1_000` in gives
    /// `L*a/(L+a) = 1e9 / 1_001_000 = 999`. Verifies outer-only accounting
    /// (USDC debit, WETH credit) and the magnitude matches the V3 fixture
    /// exactly — proving the V4 path delegates to the shared helper.
    #[test]
    fn v4_single_hop_happy_path_hooks_zero() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let v4 = AmmVenue::UniswapV4 {
            chain: ChainId::ethereum_mainnet(),
            pool_id: "0x0000000000000000000000000000000000000000000000000000000000000001".into(),
            pool_manager: Address::from_str("0x000000000004444c5dc75cb358380d2e3de08a90").unwrap(),
            hooks: Address::ZERO,
        };
        let q96 = U256::from(1u64) << 96;
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: q96,
            tick: 0,
            liquidity: policy_state::primitives::U128::from(1_000_000u64),
            ticks: vec![],
        };
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::from(900u64),
            usdc_ref(),
            weth_ref(),
            v4,
            pool_state,
        );
        let delta = swap.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 2);
        let mut saw_debit = false;
        let mut saw_credit = false;
        for tc in &delta.token_changes {
            let TokenChange::BalanceDelta { key, delta: d } = tc else {
                panic!("expected BalanceDelta, got {tc:?}");
            };
            if key == &usdc_ref().key {
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "1000");
                saw_debit = true;
            } else if key == &weth_ref().key {
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "999");
                saw_credit = true;
            } else {
                panic!("unexpected key {key:?}");
            }
        }
        assert!(saw_debit && saw_credit);
    }

    /// `UnsupportedProtocol { protocol: "uniswap_v4_with_hooks" }` *before*
    /// hooks can override `beforeSwap` / `afterSwap` curves and fees, so
    /// the reducer cannot soundly simulate them without a known-hook
    /// registry. A pool snapshot with zero liquidity would *otherwise*
    /// surface as `Invariant`, so this fixture proves the hooks short-
    /// circuit fires first.
    #[test]
    fn v4_hop_with_hooks_returns_unsupported_protocol() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let v4_with_hooks = AmmVenue::UniswapV4 {
            chain: ChainId::ethereum_mainnet(),
            pool_id: "0x0000000000000000000000000000000000000000000000000000000000000001".into(),
            pool_manager: Address::from_str("0x000000000004444c5dc75cb358380d2e3de08a90").unwrap(),
            // Non-zero hooks address — deliberate, must trigger the
            // unsupported-protocol short-circuit regardless of pool state.
            hooks: Address::from([0x42u8; 20]),
        };
        // Pool state would otherwise trigger an Invariant (zero liquidity);
        // if the short-circuit *didn't* fire, this test would fail with the
        // wrong error type, catching a regression where the hooks branch
        // is bypassed.
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: policy_state::primitives::U128::from(0u64),
            ticks: vec![],
        };
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::ZERO,
            usdc_ref(),
            weth_ref(),
            v4_with_hooks,
            pool_state,
        );
        let err = swap.apply(&state, &ctx()).unwrap_err();
        match err {
            ReducerError::UnsupportedProtocol { protocol, action } => {
                assert_eq!(protocol, "uniswap_v4_with_hooks");
                assert_eq!(action, "swap");
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
    }

    /// Batch 2 — Curve V2 dispatch now lands in `curve_v2::quote_swap_hop`.
    /// A degenerate 3-coin payload with zero `price_scale` is rejected
    /// upstream by the twocrypto simplification (`needs 2 coins` Invariant),
    /// so the observable error type changes from `UnsupportedProtocol` to
    /// `Invariant`. The test pins this to catch a regression where the Curve
    /// dispatch silently falls back to `UnsupportedProtocol`.
    #[test]
    fn curve_v2_three_coin_payload_surfaces_invariant_from_helper() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let curve = AmmVenue::CurveV2 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0xd51a44d3fae010294c616388b506acda1bfaae46").unwrap(),
        };
        let pool_state = PoolState::Cryptoswap {
            balances: vec![U256::ZERO; 3],
            price_scale: vec![U256::ZERO; 2],
            a_gamma: U256::ZERO,
            fee_bp: 4,
        };
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::ZERO,
            usdc_ref(),
            weth_ref(),
            curve,
            pool_state,
        );
        let err = swap.apply(&state, &ctx()).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("2 coins")),
            "unexpected err: {err:?}"
        );
    }

    /// Batch 2 — Curve V1 happy path through `swap.rs`. Two-coin `StableSwap`
    /// pool at equal balances (`1_000_000` / `1_000_000`), `A = 100`, zero
    /// fee → approximately 1:1. Verifies outer-only accounting and that the
    /// venue dispatch lands in the `curve_v1` helper. Slippage threshold
    /// tuned ≤ 999 to absorb the Newton-iteration +/- 2-wei slack documented
    /// in the `curve_v1` unit tests.
    #[test]
    fn curve_v1_single_hop_happy_path_equal_balance_peg() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let curve = AmmVenue::CurveV1 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7").unwrap(),
            n_coins: 2,
            is_meta: false,
        };
        let pool_state = PoolState::StableV1 {
            balances: vec![U256::from(1_000_000u64), U256::from(1_000_000u64)],
            a: 100,
            fee_bp: 0,
        };
        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::from(990u64),
            usdc_ref(),
            weth_ref(),
            curve,
            pool_state,
        );
        let delta = swap.apply(&state, &ctx()).unwrap();
        // Outer-only accounting (USDC debit, WETH credit).
        assert_eq!(delta.token_changes.len(), 2);
    }

    /// Batch 2 — Balancer V2 happy path through `swap.rs`. 50/50 weighted
    /// pool (`1_000 / 1_000`, zero fee, `100` in) → `90` out via the
    /// balanced-weight closed form. Verifies the dispatch lands in
    /// `balancer_v2`.
    #[test]
    fn balancer_v2_single_hop_happy_path_5050_weighted() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let bal = AmmVenue::BalancerV2 {
            chain: ChainId::ethereum_mainnet(),
            vault: Address::from_str("0xba12222222228d8ba445958a75a0704d566bf2c8").unwrap(),
            pool_id: format!("0x{}", "00".repeat(32)),
            pool_type: crate::action::amm::BalancerPoolType::Weighted,
        };
        let pool_state = PoolState::Weighted {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            weights: vec![50, 50],
            fee_bp: 0,
        };
        let swap = exact_in_swap(
            U256::from(100u64),
            U256::from(80u64),
            usdc_ref(),
            weth_ref(),
            bal,
            pool_state,
        );
        let delta = swap.apply(&state, &ctx()).unwrap();
        let weth_change = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if key == &weth_ref().key && delta.is_positive() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected positive WETH delta");
        assert_eq!(weth_change, "90");
    }

    // ----------------------------------------------------------------------
    // Missing token_out holding → TokenNotFound (PDF §11 swap fixtures
    // pre-seed both holdings; verify the helper error surfaces).
    // ----------------------------------------------------------------------

    /// If the user has no `token_out` holding (not even zero balance), the
    /// `helpers::balance::credit` call propagates `TokenNotFound`. The
    /// fixture-side PDF §11 setup must pre-seed a zero-balance holding for the
    /// receive-side; this test documents the failure mode when that is
    /// missing.
    #[test]
    fn missing_token_out_holding_returns_token_not_found() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(
            usdc_ref().key,
            make_holding(&usdc_ref(), U256::from(1_000_000u64), "USDC"),
        );
        // weth holding intentionally absent

        let swap = exact_in_swap(
            U256::from(1_000u64),
            U256::ZERO,
            usdc_ref(),
            weth_ref(),
            v2_venue(),
            xy_pool(10_000, 10_000, 30),
        );
        let err = swap.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    // ----------------------------------------------------------------------
    // ----------------------------------------------------------------------

    fn one_inch_router_addr() -> Address {
        Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap()
    }

    fn aggregator_venue() -> AmmVenue {
        AmmVenue::AggregatorRoute {
            chain: ChainId::ethereum_mainnet(),
            router: one_inch_router_addr(),
            route_hash: format!("0x{}", "00".repeat(32)),
            executor: None,
        }
    }

    fn make_aggregator_meta(permit_bundled: bool) -> AggregatorMeta {
        AggregatorMeta {
            aggregator: AggregatorKind::OneInchV6,
            router: one_inch_router_addr(),
            executor: Some(one_inch_router_addr()),
            raw_calldata_hash: format!("0x{}", "00".repeat(32)),
            permit_bundled,
            referrer: None,
            referrer_fee_bp: 0,
        }
    }

    fn aggregator_swap_action(amount_in: U256, min_out: U256, meta: AggregatorMeta) -> SwapAction {
        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10_000,
                hops: vec![RouteHop {
                    token_in: usdc_ref(),
                    token_out: weth_ref(),
                    venue: v2_venue(),
                    pool_state: xy_pool(10_000, 10_000, 30),
                    effective_fee_bp: 30,
                    estimated_out: U256::ZERO,
                }],
                estimated_out: U256::ZERO,
            }],
            aggregator: Some(meta),
        };
        SwapAction {
            venue: aggregator_venue(),
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: weth_ref(),
                direction: SwapDirection::ExactInput {
                    amount_in,
                    min_amount_out: min_out,
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: make_live_inputs(route),
        }
    }

    /// `AggregatorRoute` happy path with `permit_bundled = true`. The
    /// resulting `StateDelta` must contain exactly three rows in the
    /// `permit → debit → credit` order: a `Permit2`-shaped `ApprovalSet`
    /// on `token_in`, then the USDC debit, then the WETH credit. The
    /// underlying hop math is identical to the V2 happy path (`1_000` in,
    /// `10_000` / `10_000` reserves, 30 bp fee → 906 out).
    #[test]
    fn aggregator_route_with_permit_bundle_happy_path() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let meta = make_aggregator_meta(true);
        let swap = aggregator_swap_action(U256::from(1_000u64), U256::from(800u64), meta);

        let delta = swap.apply(&state, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 3);
        // Order matters — permit comes first, then debit, then credit.
        match &delta.token_changes[0] {
            TokenChange::ApprovalSet { key, .. } => {
                assert_eq!(*key, usdc_ref().key);
            }
            other => panic!("expected ApprovalSet first, got {other:?}"),
        }
        match &delta.token_changes[1] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "1000");
            }
            other => panic!("expected USDC debit, got {other:?}"),
        }
        match &delta.token_changes[2] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, weth_ref().key);
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "906");
            }
            other => panic!("expected WETH credit, got {other:?}"),
        }
    }

    /// `permit_bundled = false` → no `ApprovalSet`, just the two balance
    /// changes (matches the V2 single-hop happy path output exactly).
    #[test]
    fn aggregator_route_without_permit_bundle_emits_only_balance_changes() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let meta = make_aggregator_meta(false);
        let swap = aggregator_swap_action(U256::from(1_000u64), U256::from(800u64), meta);

        let delta = swap.apply(&state, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 2);
        for tc in &delta.token_changes {
            assert!(
                matches!(tc, TokenChange::BalanceDelta { .. }),
                "expected BalanceDelta only, got {tc:?}"
            );
        }
    }

    /// `AggregatorRoute` with an unknown executor surfaces as `Invariant`
    /// *before* any pool math or balance accounting runs — proving the
    /// outer-level hook fires before the route walk's hop side effects
    /// would land in `delta`.
    #[test]
    fn aggregator_route_unknown_executor_rejected() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let mut meta = make_aggregator_meta(false);
        meta.executor =
            Some(Address::from_str("0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef").unwrap());
        let swap = aggregator_swap_action(U256::from(1_000u64), U256::ZERO, meta);

        let err = swap.apply(&state, &ctx()).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("unknown executor")),
            "unexpected err: {err:?}"
        );
    }

    /// Inner-hop `AggregatorRoute` (an aggregator venue inside another
    /// aggregator dispatch.
    #[test]
    fn nested_aggregator_inner_hop_returns_unsupported_protocol() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let inner_agg = AmmVenue::AggregatorRoute {
            chain: ChainId::ethereum_mainnet(),
            router: one_inch_router_addr(),
            route_hash: format!("0x{}", "00".repeat(32)),
            executor: None,
        };
        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10_000,
                hops: vec![RouteHop {
                    token_in: usdc_ref(),
                    token_out: weth_ref(),
                    venue: inner_agg,
                    pool_state: xy_pool(10_000, 10_000, 30),
                    effective_fee_bp: 30,
                    estimated_out: U256::ZERO,
                }],
                estimated_out: U256::ZERO,
            }],
            aggregator: None,
        };
        let swap = SwapAction {
            venue: v2_venue(),
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: weth_ref(),
                direction: SwapDirection::ExactInput {
                    amount_in: U256::from(1_000u64),
                    min_amount_out: U256::ZERO,
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: make_live_inputs(route),
        };

        let err = swap.apply(&state, &ctx()).unwrap_err();
        match err {
            ReducerError::UnsupportedProtocol { protocol, action } => {
                assert_eq!(protocol, "aggregator_route");
                assert_eq!(action, "swap");
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
    }
}
