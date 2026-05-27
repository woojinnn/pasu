//! `SwapAction` reducer — single-pool or routed token-for-token swap.
//!
//! ## Phase 2D scope
//!
//! Wires up the **route walk** and **venue dispatch**. Only the V2-family
//! venues (`UniswapV2`, `SushiV2`) are implemented today; concentrated-liquidity
//! (V3 / V4), aggregator outer-level handling, and the remaining stable /
//! weighted / book pools surface as `UnsupportedProtocol` until their per-phase
//! activation (Phase 2E / 2F / 2G — see plan).
//!
//! ## Balance accounting
//!
//! Only the *outer* `token_in` / `token_out` are debited / credited. Hops in a
//! multi-hop path move strictly *inside* the swap envelope (router escrow); the
//! user's wallet only sees the first leg's spend and the last leg's receipt.
//! This matches the user-visible accounting convention of every routed-swap
//! protocol (Uniswap routers, 1inch, 0x, Paraswap): intermediate hop tokens
//! are not credited to the user.
//!
//! ## Slippage check
//!
//! `ExactInput` enforces `total_out >= min_amount_out` before any state
//! mutation is recorded. `ExactOutput` simulation reverses that bound (must
//! return at least `amount_out` and consume at most `max_amount_in`); the
//! Phase 2D body wires the `ExactInput` branch only — exact-output simulation
//! requires inverse hop math (`getAmountIn`) which we defer with the rest of
//! the V3 cases.

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::{AmmVenue, SwapAction, SwapDirection};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::uniswap_v2;

impl Reducer for SwapAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let route = &self.live_inputs.route.value;

        // Phase 2D: only ExactInput is fully simulated. ExactOutput would need
        // a per-venue `getAmountIn` quote (V2 inverse formula, V3 backward
        // sweep, …); for now we use the user-signed `max_amount_in` as the
        // spend cap and return a Phase 2E follow-up via Invariant if any path
        // ends up needing the exact-output inverse math.
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
                    AmmVenue::UniswapV2 { .. } | AmmVenue::SushiV2 { .. } => {
                        uniswap_v2::quote_swap_hop(state, ctx, self, &hop.pool_state, hop_in)?
                    }
                    // Phase 2E — V3 concentrated-liquidity math.
                    AmmVenue::UniswapV3 { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "uniswap_v3".into(),
                        });
                    }
                    // Phase 2F — V4 singleton + hooks.
                    AmmVenue::UniswapV4 { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "uniswap_v4".into(),
                        });
                    }
                    // Phase 2 follow-up batches — Curve, Balancer, Trader Joe,
                    // Maverick. Surface each one with a distinct protocol tag
                    // so policy / observability can target them individually.
                    AmmVenue::CurveV1 { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "curve_v1".into(),
                        });
                    }
                    AmmVenue::CurveV2 { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "curve_v2".into(),
                        });
                    }
                    AmmVenue::BalancerV2 { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "balancer_v2".into(),
                        });
                    }
                    AmmVenue::BalancerV3 { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "balancer_v3".into(),
                        });
                    }
                    AmmVenue::TraderJoeLB { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "trader_joe_lb".into(),
                        });
                    }
                    AmmVenue::MaverickV2 { .. } => {
                        return Err(ReducerError::UnsupportedProtocol {
                            action: "swap".into(),
                            protocol: "maverick_v2".into(),
                        });
                    }
                    // Phase 2G — aggregator outer-level wiring. The aggregator
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

        // Outer-level aggregator hook (Phase 2G) — referrer fees, executor
        // bookkeeping, permit bundling. Intentionally omitted in 2D.

        let mut delta = StateDelta::new();
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
        AmmVenue, PoolState, RouteHop, RoutePath, SwapDirection, SwapLiveInputs, SwapParams,
        SwapRoute,
    };
    use simulation_state::delta::TokenChange;
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::primitives::{Address, ChainId, Time, U256};
    use simulation_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use simulation_state::wallet::WalletId;
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
        let route = single_hop_route(token_in.clone(), token_out.clone(), venue.clone(), pool_state);
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
    // ExactOutput direction — Phase 2D uses max_amount_in as the spend cap
    // ----------------------------------------------------------------------

    /// `ExactOutput` direction is currently simulated as if `max_amount_in`
    /// were the spent amount (no inverse hop math). The output uses the same
    /// constant-product quote — Phase 2D test pins the current behaviour
    /// (deferred exact-output quote) so a Phase 2E inverse-math change is
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

    /// A V3 hop (concentrated liquidity) must surface as `UnsupportedProtocol
    /// { action: "swap", protocol: "uniswap_v3" }` until Phase 2E lands.
    #[test]
    fn v3_hop_returns_unsupported_protocol() {
        let state = state_with_pair(U256::from(1_000_000u64), U256::ZERO);
        let v3 = AmmVenue::UniswapV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: simulation_state::primitives::U128::from(0u64),
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
        match err {
            ReducerError::UnsupportedProtocol { action, protocol } => {
                assert_eq!(action, "swap");
                assert_eq!(protocol, "uniswap_v3");
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
    }

    /// Curve hop also surfaces as `UnsupportedProtocol` with the appropriate
    /// protocol tag.
    #[test]
    fn curve_hop_returns_unsupported_protocol_with_curve_tag() {
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
        match err {
            ReducerError::UnsupportedProtocol { protocol, .. } => {
                assert_eq!(protocol, "curve_v2");
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
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
}
