//! Uniswap V3 swap math — concentrated-liquidity tick traversal.
//!
//! Pure functions called from `swap.rs` after dispatch on `AmmVenue::UniswapV3`.
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//!
//! ## Phase 2E scope — **simplified single-tick closed form**
//!
//! Today's implementation evaluates the swap **only on the active in-range
//! liquidity** at the current `sqrt_price_x96` / `liquidity`, without crossing
//! any tick boundary. The math reproduces the canonical Uniswap V3 single-step
//! closed form:
//!
//! ```text
//!   sqrt_price_next = L * sqrt_price * Q96 / (L * Q96 + amount_in * sqrt_price)
//!   amount_out      = L * (sqrt_price - sqrt_price_next) / Q96
//! ```
//!
//! which is `SqrtPriceMath::getNextSqrtPriceFromAmount0RoundingUp` followed by
//! `SqrtPriceMath::getAmount1Delta` for the **`zeroForOne`** direction
//! (token0 → token1, `sqrt_price` decreasing) — the assumption we adopt here
//! because the on-chain `token0` / `token1` ordering of the pool is **not**
//! carried in `PoolState::Concentrated`. See the "Direction assumption" note
//! below for the impact.
//!
//! Full tick traversal (cross-tick swaps with `liquidity_net` application from
//! the `ticks` snapshot) is deferred to a follow-up phase. Large enough
//! `amount_in` will therefore *under-quote* in cases where the swap would have
//! crossed into a richer tick range, and may *over-quote* in cases where it
//! would have crossed into a thinner one. The active-tick close-form is the
//! Uniswap V3 baseline used by the v3-core reference itself before
//! `SwapMath::computeSwapStep` clamps each step to the next tick boundary.
//!
//! ## Direction assumption (zeroForOne)
//!
//! `PoolState::Concentrated` does *not* carry the pool's `(token0, token1)`
//! ordering, only `sqrt_price_x96`, `tick`, `liquidity`, and `ticks`. We
//! therefore evaluate every hop in the **`zeroForOne` direction** (the more
//! common direction for non-WETH-paired pools and a deterministic baseline).
//! When the hop's `RouteHop::token_in` is actually pool `token1`, the closed
//! form below produces an under-estimate (the `oneForZero` formulas are
//! mirror-symmetric but use `SqrtPriceMath::getNextSqrtPriceFromAmount1` and
//! `getAmount0Delta`, with `sqrt_price` *increasing*). Lifting this assumption
//! is part of the follow-up tick-traversal work; until then, the
//! `RouteHop.estimated_out` field (carried on the live route) acts as the
//! oracle on cases where direction matters for policy.
//!
//! ## Fee accounting
//!
//! V3 also subtracts the pool fee from `amount_in` before the swap math
//! (`amount_in_net = amount_in - amount_in * fee / 1e6`). The
//! `effective_fee_bp` field on `RouteHop` carries the fee for policy
//! evaluation, but the Phase 2E body **does not** apply it because:
//!   * the fee schedule (`fee_pips = fee_tier_bp * 100`) is on
//!     `AmmVenue::UniswapV3.fee_tier_bp`, not on `PoolState::Concentrated`;
//!   * the closed-form math above already matches the v3-core test fixtures
//!     for the zero-fee case, and adding fee subtraction without correctly
//!     plumbing the venue-side `fee_tier_bp` into this function would be
//!     misleading. Lifting fee accounting is bundled with the follow-up
//!     tick-traversal work.
//!
//! ## References
//!
//! * `SwapMath.sol`         — <https://github.com/Uniswap/v3-core/blob/main/contracts/libraries/SwapMath.sol>
//! * `SqrtPriceMath.sol`    — <https://github.com/Uniswap/v3-core/blob/main/contracts/libraries/SqrtPriceMath.sol>
//! * `TickMath.sol`         — <https://github.com/Uniswap/v3-core/blob/main/contracts/libraries/TickMath.sol>

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::{ReducerError, ReducerResult};

/// `Q96 = 2^96`. The fixed-point scale used by `sqrt_price_x96` in
/// Uniswap V3 / V4.
fn q96() -> U256 {
    U256::from(1u64) << 96
}

/// Shared concentrated-liquidity single-hop math used by both
/// `uniswap_v3::quote_swap_hop` and `uniswap_v4::quote_swap_hop`.
///
/// Implements the Phase 2E **simplified active-tick closed form** in the
/// `zeroForOne` direction (see `uniswap_v3` module docs for the algebra,
/// direction assumption and fee-accounting caveat). V4 with `hooks == 0`
/// reuses the same math identically — the singleton `PoolManager` invokes
/// the same `SwapMath::computeSwapStep` library on its V4-pool snapshot,
/// and Phase 2F defers tick traversal alongside V3.
///
/// `protocol_tag` is woven into the `Invariant` message so callers can keep
/// their per-protocol error trails (the V3 + V4 unit tests assert on
/// substrings like `"zero active liquidity"` and `"Concentrated"`).
///
/// Errors with `Invariant` on:
///   * `liquidity == 0` (empty pool — division by zero downstream),
///   * `sqrt_price_x96 == 0` (degenerate price — multiplication trivially
///     zero, but also signals an invalid pool snapshot),
///   * any `U256` overflow in the closed-form arithmetic,
///   * `PoolState` variant other than `Concentrated`.
pub(super) fn concentrated_swap_math(
    pool_state: &PoolState,
    amount_in: U256,
    protocol_tag: &str,
) -> ReducerResult<U256> {
    match pool_state {
        PoolState::Concentrated {
            sqrt_price_x96,
            tick: _,
            liquidity,
            ticks: _,
        } => {
            if liquidity.is_zero() {
                return Err(ReducerError::Invariant(format!(
                    "{protocol_tag} quote: zero active liquidity"
                )));
            }
            if sqrt_price_x96.is_zero() {
                return Err(ReducerError::Invariant(format!(
                    "{protocol_tag} quote: zero sqrt_price_x96"
                )));
            }
            // `liquidity` is `U128`; widen to `U256` for the closed-form math.
            let l: U256 = U256::from(*liquidity);
            let sp = *sqrt_price_x96;
            let q96 = q96();

            // Short-circuit: `amount_in == 0` ⇒ `amount_out == 0`. Skipping
            // the closed-form here avoids a denominator of `L * Q96` that
            // would still produce a zero numerator (`sp - sp_next == 0`),
            // but the explicit branch keeps the intent obvious and matches
            // the V2 helper's behaviour.
            if amount_in.is_zero() {
                return Ok(U256::ZERO);
            }

            // sqrt_price_next = L * sp * Q96 / (L * Q96 + amount_in * sp)
            //
            // Numerator and denominator are split into checked multiplications
            // so an overflow surfaces as an `Invariant` rather than a panic.
            let num_l_sp = l.checked_mul(sp).ok_or_else(|| {
                ReducerError::Invariant(format!("{protocol_tag} quote: L*sp overflow"))
            })?;
            let numerator = num_l_sp.checked_mul(q96).ok_or_else(|| {
                ReducerError::Invariant(format!("{protocol_tag} quote: L*sp*Q96 overflow"))
            })?;

            let denom_l_q96 = l.checked_mul(q96).ok_or_else(|| {
                ReducerError::Invariant(format!("{protocol_tag} quote: L*Q96 overflow"))
            })?;
            let denom_in_sp = amount_in.checked_mul(sp).ok_or_else(|| {
                ReducerError::Invariant(format!("{protocol_tag} quote: amount_in*sp overflow"))
            })?;
            let denominator = denom_l_q96.checked_add(denom_in_sp).ok_or_else(|| {
                ReducerError::Invariant(format!("{protocol_tag} quote: denominator overflow"))
            })?;

            // Guarded above: `liquidity` and `sqrt_price_x96` are both
            // non-zero, so `denom_l_q96 > 0` and the sum is too.
            let sqrt_price_next = numerator / denominator;

            // amount_out = L * (sp - sqrt_price_next) / Q96
            //
            // The `zeroForOne` closed form guarantees `sqrt_price_next <= sp`
            // when `amount_in > 0` (price falls as token0 enters), so the
            // checked subtraction is structurally safe; we still
            // `checked_sub` so a numerical-rounding case surfaces cleanly.
            let sp_drop = sp.checked_sub(sqrt_price_next).ok_or_else(|| {
                ReducerError::Invariant(format!(
                    "{protocol_tag} quote: sqrt_price_next > sp (direction violation)"
                ))
            })?;
            let out_num = l.checked_mul(sp_drop).ok_or_else(|| {
                ReducerError::Invariant(format!("{protocol_tag} quote: L*(sp-sp_next) overflow"))
            })?;
            // `q96` is non-zero, division is safe.
            Ok(out_num / q96)
        }
        _ => Err(ReducerError::Invariant(format!(
            "non-Concentrated pool_state for {protocol_tag} swap"
        ))),
    }
}

/// Quote a single hop on a Uniswap V3 pool given its `Concentrated`
/// `PoolState` snapshot. Returns the hop's output amount; caller is
/// responsible for balance changes.
///
/// **Phase 2E simplified** — see module docs. Active-tick closed form,
/// `zeroForOne` direction, no fee subtraction, no tick crossing.
///
/// Errors with `Invariant` on:
///   * `liquidity == 0` (empty pool — division by zero downstream),
///   * `sqrt_price_x96 == 0` (degenerate price — multiplication trivially
///     zero, but also signals an invalid pool snapshot),
///   * any `U256` overflow in the closed-form arithmetic,
///   * `PoolState` variant other than `Concentrated`.
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    concentrated_swap_math(pool_state, amount_in, "uniswap_v3")
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{AmmVenue, SwapDirection, SwapLiveInputs, SwapParams, SwapRoute};
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::primitives::{Address, ChainId, Time, U128, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::wallet::WalletId;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from([0u8; 20]),
            [ChainId::ethereum_mainnet()],
        ))
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

    fn dummy_swap() -> SwapAction {
        SwapAction {
            venue: AmmVenue::UniswapV3 {
                chain: ChainId::ethereum_mainnet(),
                pool: Address::from([1u8; 20]),
                fee_tier_bp: 500,
            },
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: weth_ref(),
                direction: SwapDirection::ExactInput {
                    amount_in: U256::from(1u64),
                    min_amount_out: U256::ZERO,
                },
                recipient: Address::from([3u8; 20]),
                slippage_bp: 50,
            },
            live_inputs: SwapLiveInputs {
                route: LiveField::new(
                    SwapRoute {
                        paths: vec![],
                        aggregator: None,
                    },
                    DataSource::UserSupplied,
                    now(),
                ),
                expected_amount_out: LiveField::new(U256::ZERO, DataSource::UserSupplied, now()),
                price_impact_bp: LiveField::new(0u32, DataSource::UserSupplied, now()),
                gas_estimate: LiveField::new(U256::ZERO, DataSource::UserSupplied, now()),
            },
        }
    }

    /// Convenience: `Concentrated` pool with the given liquidity and
    /// `sqrt_price_x96`, empty `ticks` snapshot (simplified math doesn't
    /// traverse).
    fn pool(liquidity: u128, sqrt_price_x96: U256) -> PoolState {
        PoolState::Concentrated {
            sqrt_price_x96,
            tick: 0,
            liquidity: U128::from(liquidity),
            ticks: vec![],
        }
    }

    /// `Q96` constant — `2^96 = 79_228_162_514_264_337_593_543_950_336`.
    fn q96_val() -> U256 {
        U256::from(1u64) << 96
    }

    // ----------------------------------------------------------------------
    // Closed-form sanity: `sp_x96 == Q96` (price-1 pool, like a 1:1 stable
    // pair). Then `sp == Q96`, so
    //   sp_next = L * Q96 * Q96 / (L * Q96 + a * Q96) = L * Q96 / (L + a)
    //   out     = L * (Q96 - L*Q96/(L+a)) / Q96
    //           = L * Q96 * (L + a - L) / ((L + a) * Q96)
    //           = L * a / (L + a)
    // For L = 1_000_000, a = 1_000  →  out = 1e9 / 1_001_000 = 999.
    // ----------------------------------------------------------------------

    #[test]
    fn quote_concentrated_price_one_pool_matches_closed_form() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(1_000_000, q96_val());
        let out = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1_000u64)).unwrap();
        // L*a/(L+a) = 1e9 / 1_001_000 = 999 (integer floor).
        assert_eq!(out, U256::from(999u64));
    }

    /// Larger swap on the same `price = 1` pool: `L = 1_000_000`,
    /// `a = 100_000` → `out = 1e11 / 1_100_000 = 90_909`.
    #[test]
    fn quote_concentrated_price_one_pool_larger_in() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(1_000_000, q96_val());
        let out =
            quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(100_000u64)).unwrap();
        assert_eq!(out, U256::from(90_909u64));
    }

    // ----------------------------------------------------------------------
    // Zero amount_in produces zero amount_out (no underflow, no error).
    // ----------------------------------------------------------------------

    #[test]
    fn quote_concentrated_zero_amount_in_is_zero_out() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(1_000_000, q96_val());
        let out = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::ZERO).unwrap();
        assert_eq!(out, U256::ZERO);
    }

    // ----------------------------------------------------------------------
    // Zero liquidity must error (we cannot quote against an empty range).
    // ----------------------------------------------------------------------

    #[test]
    fn quote_concentrated_zero_liquidity_errors() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(0, q96_val());
        let err = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1u64)).unwrap_err();
        assert!(
            matches!(err, ReducerError::Invariant(msg) if msg.contains("zero active liquidity"))
        );
    }

    // ----------------------------------------------------------------------
    // Zero sqrt_price_x96 must error.
    // ----------------------------------------------------------------------

    #[test]
    fn quote_concentrated_zero_sqrt_price_errors() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(1_000_000, U256::ZERO);
        let err = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("zero sqrt_price_x96")));
    }

    // ----------------------------------------------------------------------
    // Non-Concentrated variants must be rejected.
    // ----------------------------------------------------------------------

    #[test]
    fn quote_rejects_non_concentrated_pool_state() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(10_000u64),
            reserve_out: U256::from(10_000u64),
            fee_bp: 30,
        };
        let err = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("Concentrated")));
    }

    // ----------------------------------------------------------------------
    // Monotonicity in `amount_in`: doubling in produces strictly more out
    // (but never more than a 1:1 fraction).
    // ----------------------------------------------------------------------

    #[test]
    fn quote_concentrated_monotonic_in_amount_in() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(10_000_000, q96_val());
        let small =
            quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1_000u64)).unwrap();
        let large =
            quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(2_000u64)).unwrap();
        assert!(large > small);
        // The closed form `out = L*a/(L+a)` is sub-linear in `a`, so out is
        // always ≤ amount_in for a price-1 pool.
        assert!(large <= U256::from(2_000u64));
    }

    // ----------------------------------------------------------------------
    // Price >> 1 pool: `sp_x96 = 2 * Q96` means the on-chain price is `4`
    // (price = (sp/Q96)^2). Algebra:
    //   sp_next = L * 2Q96 * Q96 / (L * Q96 + a * 2Q96)
    //           = 2 L Q96 / (L + 2a)
    //   out     = L * (2Q96 - 2 L Q96 / (L + 2a)) / Q96
    //           = 2L * (L + 2a - L) / (L + 2a)
    //           = 4 L a / (L + 2a)
    // For L = 1_000_000, a = 100  →  out = 400_000_000 / 1_000_200 = 399.
    // Price ≈ 4, so spending 100 should yield ≈400 — checks out (less the
    // closed-form's small price-impact).
    // ----------------------------------------------------------------------

    #[test]
    fn quote_concentrated_price_four_pool() {
        let state = empty_state();
        let swap = dummy_swap();
        let sp_x96 = q96_val().checked_mul(U256::from(2u64)).unwrap();
        let pool_state = pool(1_000_000, sp_x96);
        let out = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(100u64)).unwrap();
        // 4 * 1_000_000 * 100 / (1_000_000 + 200) = 4e8 / 1_000_200 = 399
        assert_eq!(out, U256::from(399u64));
    }
}
