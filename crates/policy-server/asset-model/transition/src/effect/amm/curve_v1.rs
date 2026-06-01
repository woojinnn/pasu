//! Curve V1 swap math — stableswap invariant.
//!
//! Pure functions called from `swap.rs` after dispatch on `AmmVenue::CurveV1`.
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//!
//! ## Simplification
//!
//! `PoolState::StableV1` carries a flat `balances: Vec<U256>` plus the
//! amplification coefficient `A` and a fee in basis points. It does **not**
//! carry the swap direction `(i, j)` indices — the caller (`swap.rs`) only
//! supplies `amount_in` per hop, leaving coin selection ambient. We adopt the
//! convention that `i = 0` (sell coin) and `j = 1` (buy coin) so the helper is
//! deterministic without extending the `PoolState` schema. The first batch
//! venues (V2 / V3 / V4) likewise rely on per-hop schema (`XyConstant`,
//! `Concentrated`) that pre-selects the direction; this matches that pattern.
//!
//! ## Newton iteration bounds
//!
//! `MAX_ITER = 255` matches the Vyper implementation's hard cap. Convergence
//! tolerance is `1 wei` (`|y_new - y_prev| <= 1`), matching the on-chain
//! loop's exit condition. Non-convergence after `MAX_ITER` surfaces as
//! `Invariant("curve_v1 newton diverged")`.
//!
//! ## 1차 출처
//!
//! * Egorov 2019 — "`StableSwap` — efficient mechanism for Stablecoin liquidity"
//!   <https://classic.curve.fi/files/stableswap-paper.pdf> (§3 invariant,
//!   §4 `get_y` derivation).
//! * `Curve V1` contracts —
//!   <https://github.com/curvefi/curve-contract/blob/master/contracts/pool-templates/base/SwapTemplateBase.vy>
//!   * `_xp` (line ~248) — scaling, treated as identity here because
//!     `PoolState::StableV1` already exposes pre-scaled balances.
//!   * `get_D` (line ~260) — Newton solver for the invariant.
//!   * `get_y` (line ~292) — Newton solver for the new `y` after `x` change.
//!   * `exchange` (line ~407) — fee subtraction (`dy * fee / FEE_DENOMINATOR`,
//!     where `FEE_DENOMINATOR = 10**10`; here we adopt basis-point units —
//!     `dy * fee_bp / 10_000` — matching the `PoolState::StableV1.fee_bp`
//!     schema field declared in `action/amm.rs`).

// Phase 2 stubs: callers (per-action reducers) are still `todo!()` so these
// functions look unused. Lift this allow when the first caller wires up.
#![allow(dead_code)]

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::{ReducerError, ReducerResult};

/// Hard cap on Newton-iteration rounds — matches the on-chain Vyper loop.
const MAX_ITER: u32 = 255;

/// Solve for the `StableSwap` invariant `D` given the per-coin balances and
/// the amplification coefficient `A`.
///
/// Implements the canonical fixed-point iteration from
/// `SwapTemplateBase.vy::get_D` — see Egorov 2019, §3.
///
/// ```text
///   S    = Σ xp[i]
///   D    = S
///   loop:
///     D_P = D^(n+1) / (n^n * Π xp[i])
///     D   = (Ann*S + n*D_P) * D / ((Ann-1)*D + (n+1)*D_P)
///     break if |D_new - D_prev| <= 1
///   Ann  = A * n^n
/// ```
///
/// Returns `Invariant` on:
///   * `balances` empty (n=0),
///   * any `U256` overflow during the iteration,
///   * non-convergence after `MAX_ITER` rounds.
///
/// Shared with `balancer_v2` / `balancer_v3` so the `StableSwap`-family pool
/// types in those venues can reuse the same Vyper-equivalent solver instead
/// of duplicating it (Balancer `StablePool.sol::_calcOutGivenIn` calls the
/// same `StableSwap` invariant under the hood).
pub(super) fn compute_d(balances: &[U256], a: u32) -> ReducerResult<U256> {
    let n_u32 = u32::try_from(balances.len())
        .map_err(|_| ReducerError::Invariant("curve_v1 D: too many coins".into()))?;
    if n_u32 == 0 {
        return Err(ReducerError::Invariant("curve_v1 D: empty balances".into()));
    }
    let n = U256::from(n_u32);

    let mut s = U256::ZERO;
    for b in balances {
        s = s
            .checked_add(*b)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: S overflow".into()))?;
    }
    if s.is_zero() {
        return Ok(U256::ZERO);
    }

    // Ann = A * n^n
    let mut n_pow_n = U256::from(1u64);
    for _ in 0..n_u32 {
        n_pow_n = n_pow_n
            .checked_mul(n)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: n^n overflow".into()))?;
    }
    let ann = U256::from(a)
        .checked_mul(n_pow_n)
        .ok_or_else(|| ReducerError::Invariant("curve_v1 D: Ann overflow".into()))?;

    let mut d = s;
    for _ in 0..MAX_ITER {
        // D_P = D^(n+1) / (n^n * Π xp[i])
        //      = D * Π (D / (n * xp[i]))                  (Vyper rearranged form)
        // Vyper form (avoids huge intermediates):
        //   D_P = D
        //   for x in xp: D_P = D_P * D / (x * n)
        let mut d_p = d;
        for x in balances {
            if x.is_zero() {
                return Err(ReducerError::Invariant("curve_v1 D: zero balance".into()));
            }
            let denom = x
                .checked_mul(n)
                .ok_or_else(|| ReducerError::Invariant("curve_v1 D: x*n overflow".into()))?;
            d_p = d_p
                .checked_mul(d)
                .ok_or_else(|| ReducerError::Invariant("curve_v1 D: D_P*D overflow".into()))?
                / denom;
        }

        let d_prev = d;
        // numerator   = (Ann*S + n*D_P) * D
        // denominator = (Ann - 1)*D     + (n+1)*D_P
        let ann_s = ann
            .checked_mul(s)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: Ann*S overflow".into()))?;
        let n_dp = n
            .checked_mul(d_p)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: n*D_P overflow".into()))?;
        let num_pre = ann_s
            .checked_add(n_dp)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: num_pre overflow".into()))?;
        let num = num_pre
            .checked_mul(d)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: numerator overflow".into()))?;

        let ann_minus_one = ann
            .checked_sub(U256::from(1u64))
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: (Ann-1) underflow".into()))?;
        let term_a = ann_minus_one
            .checked_mul(d)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: (Ann-1)*D overflow".into()))?;
        let term_b = (n
            .checked_add(U256::from(1u64))
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: (n+1) overflow".into()))?)
        .checked_mul(d_p)
        .ok_or_else(|| ReducerError::Invariant("curve_v1 D: (n+1)*D_P overflow".into()))?;
        let denom = term_a
            .checked_add(term_b)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 D: denominator overflow".into()))?;
        if denom.is_zero() {
            return Err(ReducerError::Invariant(
                "curve_v1 D: zero denominator".into(),
            ));
        }

        d = num / denom;

        let diff = if d > d_prev { d - d_prev } else { d_prev - d };
        if diff <= U256::from(1u64) {
            return Ok(d);
        }
    }
    Err(ReducerError::Invariant(
        "curve_v1 D: newton diverged".into(),
    ))
}

/// Solve for the new `y` (balance of coin `j`) after coin `i` changes to `x_i`,
/// given the `StableSwap` invariant `D` and amplification `A`.
///
/// Implements `SwapTemplateBase.vy::get_y` — see Egorov 2019, §4.
///
/// ```text
///   S' = Σ xp[k] for k != j (with xp[i] replaced by x_i)
///   P' = Π xp[k] for k != j (with xp[i] replaced by x_i)
///   c  = D^(n+1) / (n^n * Ann * P')
///   b  = S' + D / Ann
///   y² + (b - D)·y - c = 0           ⇒    y = (y² + c) / (2y + b - D)
/// ```
///
/// Shared with `balancer_v2` / `balancer_v3` — see `compute_d` doc note.
#[allow(clippy::many_single_char_names)]
pub(super) fn compute_y(
    balances: &[U256],
    i: usize,
    j: usize,
    new_x_i: U256,
    a: u32,
    d: U256,
) -> ReducerResult<U256> {
    let n_u32 = u32::try_from(balances.len())
        .map_err(|_| ReducerError::Invariant("curve_v1 y: too many coins".into()))?;
    if i >= balances.len() || j >= balances.len() || i == j {
        return Err(ReducerError::Invariant(format!(
            "curve_v1 y: bad indices (i={i}, j={j}, n={n_u32})"
        )));
    }
    let n = U256::from(n_u32);

    // Ann = A * n^n
    let mut n_pow_n = U256::from(1u64);
    for _ in 0..n_u32 {
        n_pow_n = n_pow_n
            .checked_mul(n)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: n^n overflow".into()))?;
    }
    let ann = U256::from(a)
        .checked_mul(n_pow_n)
        .ok_or_else(|| ReducerError::Invariant("curve_v1 y: Ann overflow".into()))?;
    if ann.is_zero() {
        return Err(ReducerError::Invariant("curve_v1 y: zero Ann".into()));
    }

    // c = D^(n+1) / (n^n * Ann * P') accumulated in Vyper-style stream:
    //   c = D
    //   for k != j:  x_k = (k==i ? new_x_i : balances[k])
    //                S' += x_k
    //                c   = c * D / (x_k * n)
    //   c = c * D / (Ann * n)
    //   b = S' + D / Ann
    let mut s_prime = U256::ZERO;
    let mut c = d;
    for (k, b_k) in balances.iter().enumerate() {
        if k == j {
            continue;
        }
        let x_k = if k == i { new_x_i } else { *b_k };
        if x_k.is_zero() {
            return Err(ReducerError::Invariant("curve_v1 y: zero x_k".into()));
        }
        s_prime = s_prime
            .checked_add(x_k)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: S' overflow".into()))?;
        let denom = x_k
            .checked_mul(n)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: x_k*n overflow".into()))?;
        c = c
            .checked_mul(d)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: c*D overflow".into()))?
            / denom;
    }
    let denom_final = ann
        .checked_mul(n)
        .ok_or_else(|| ReducerError::Invariant("curve_v1 y: Ann*n overflow".into()))?;
    c = c
        .checked_mul(d)
        .ok_or_else(|| ReducerError::Invariant("curve_v1 y: c*D overflow".into()))?
        / denom_final;

    let b = s_prime
        .checked_add(d / ann)
        .ok_or_else(|| ReducerError::Invariant("curve_v1 y: b overflow".into()))?;

    // y = (y² + c) / (2y + b - D)
    let mut y = d;
    for _ in 0..MAX_ITER {
        let y_prev = y;
        let y_sq = y
            .checked_mul(y)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: y² overflow".into()))?;
        let num = y_sq
            .checked_add(c)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: num overflow".into()))?;
        let two_y = y
            .checked_mul(U256::from(2u64))
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: 2y overflow".into()))?;
        let two_y_plus_b = two_y
            .checked_add(b)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: 2y+b overflow".into()))?;
        let denom = two_y_plus_b
            .checked_sub(d)
            .ok_or_else(|| ReducerError::Invariant("curve_v1 y: 2y+b-D underflow".into()))?;
        if denom.is_zero() {
            return Err(ReducerError::Invariant(
                "curve_v1 y: zero denominator".into(),
            ));
        }
        y = num / denom;

        let diff = if y > y_prev { y - y_prev } else { y_prev - y };
        if diff <= U256::from(1u64) {
            return Ok(y);
        }
    }
    Err(ReducerError::Invariant(
        "curve_v1 y: newton diverged".into(),
    ))
}

/// Quote a single hop on a Curve V1 pool given its `StableV1` `PoolState`
/// snapshot. Returns the hop's output amount; caller is responsible for fee
/// accounting and balance changes.
///
/// **Direction convention** — `i = 0` (sell coin), `j = 1` (buy coin). See
/// module docs for why this is fixed at the helper level instead of being
/// passed in.
///
/// Errors with `Invariant` on:
///   * `PoolState` variant other than `StableV1`,
///   * `balances.len() < 2`,
///   * `fee_bp >= 10_000`,
///   * any Newton-iteration divergence,
///   * any `U256` overflow.
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    let PoolState::StableV1 {
        balances,
        a,
        fee_bp,
    } = pool_state
    else {
        return Err(ReducerError::Invariant(
            "non-StableV1 pool_state for curve_v1 swap".into(),
        ));
    };
    if balances.len() < 2 {
        return Err(ReducerError::Invariant(format!(
            "curve_v1 needs >= 2 coins, got {}",
            balances.len()
        )));
    }
    if *fee_bp >= 10_000 {
        return Err(ReducerError::Invariant(format!(
            "curve_v1 fee_bp {fee_bp} out of range (must be < 10000)"
        )));
    }
    if amount_in.is_zero() {
        return Ok(U256::ZERO);
    }

    let d = compute_d(balances, *a)?;
    let new_x_i = balances[0]
        .checked_add(amount_in)
        .ok_or_else(|| ReducerError::Invariant("curve_v1: x_i + amount_in overflow".into()))?;
    let new_y = compute_y(balances, 0, 1, new_x_i, *a, d)?;
    // Guard against numerical jitter — Curve subtracts an extra `1` before
    // computing `dy` in the on-chain template to compensate for rounding.
    let new_y_minus_one = new_y
        .checked_sub(U256::from(1u64))
        .ok_or_else(|| ReducerError::Invariant("curve_v1: new_y < 1 (underflow)".into()))?;
    let dy_pre_fee = balances[1]
        .checked_sub(new_y_minus_one)
        .ok_or_else(|| ReducerError::Invariant("curve_v1: balance[j] < new_y (no out)".into()))?;

    // dy_fee = dy_pre_fee * fee_bp / 10_000  (basis-point convention — see
    // module docs note on `FEE_DENOMINATOR` vs `fee_bp`).
    let fee_amt = dy_pre_fee
        .checked_mul(U256::from(*fee_bp))
        .ok_or_else(|| ReducerError::Invariant("curve_v1: fee numerator overflow".into()))?
        / U256::from(10_000u32);

    let dy = dy_pre_fee
        .checked_sub(fee_amt)
        .ok_or_else(|| ReducerError::Invariant("curve_v1: dy - fee underflow".into()))?;
    Ok(dy)
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Two-coin pool at equal balances with `A = 100`, `fee_bp = 0`. A small
    /// trade vs. an equal-balance pool should produce nearly 1:1 (stableswap's
    /// promise around the peg). With `1_000_000 / 1_000_000` balances and
    /// `amount_in = 1_000`, the closed-form output stays within a few wei of
    /// `1_000` because the curve is locally flat at the peg.
    #[test]
    fn equal_balance_small_trade_approximately_one_to_one() {
        let bal = vec![U256::from(1_000_000u64), U256::from(1_000_000u64)];
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &PoolState::StableV1 {
                balances: bal,
                a: 100,
                fee_bp: 0,
            },
            U256::from(1_000u64),
        )
        .unwrap();
        // 1 wei tolerance for Newton convergence + the on-chain `-1` rounding.
        let target = U256::from(1_000u64);
        let diff = if out > target {
            out - target
        } else {
            target - out
        };
        assert!(diff <= U256::from(2u64), "out = {out}, expected ≈ 1_000");
    }

    /// Fee subtraction works: with `fee_bp = 30` (30 bp) on the same equal
    /// pool, the output drops by ≈ 0.3 %.
    #[test]
    fn fee_subtracts_basis_points_from_pre_fee_dy() {
        let bal = vec![U256::from(1_000_000u64), U256::from(1_000_000u64)];
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &PoolState::StableV1 {
                balances: bal,
                a: 100,
                fee_bp: 30,
            },
            U256::from(1_000u64),
        )
        .unwrap();
        // ~ 1_000 - 0.3% = 997. Allow a small (≤ 2 wei) Newton slack.
        assert!(
            out <= U256::from(998u64) && out >= U256::from(996u64),
            "out = {out}"
        );
    }

    /// Zero `amount_in` short-circuits to zero out.
    #[test]
    fn zero_amount_in_returns_zero() {
        let bal = vec![U256::from(1_000_000u64), U256::from(1_000_000u64)];
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &PoolState::StableV1 {
                balances: bal,
                a: 100,
                fee_bp: 0,
            },
            U256::ZERO,
        )
        .unwrap();
        assert_eq!(out, U256::ZERO);
    }

    /// Out-of-range `fee_bp` (>= `10_000`) is rejected with `Invariant`.
    #[test]
    fn oversized_fee_errors() {
        let bal = vec![U256::from(1_000_000u64), U256::from(1_000_000u64)];
        let err = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &PoolState::StableV1 {
                balances: bal,
                a: 100,
                fee_bp: 10_000,
            },
            U256::from(1u64),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("fee_bp")));
    }

    /// Non-`StableV1` variants are rejected.
    #[test]
    fn rejects_non_stable_v1_pool_state() {
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(1u64),
            reserve_out: U256::from(1u64),
            fee_bp: 0,
        };
        let err = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool_state,
            U256::from(1u64),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("StableV1")));
    }

    /// `compute_d` returns `0` on the trivial zero-balance pool.
    #[test]
    fn compute_d_handles_zero_pool() {
        let d = compute_d(&[U256::ZERO, U256::ZERO], 100).unwrap();
        assert_eq!(d, U256::ZERO);
    }

    // -------- test fixtures --------

    use crate::action::amm::{AmmVenue, SwapDirection, SwapLiveInputs, SwapParams, SwapRoute};
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::primitives::{Address, ChainId, Time};
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

    fn dai_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0x6b175474e89094c44da98b954eedeac495271d0f").unwrap(),
            },
        }
    }

    fn dummy_swap() -> SwapAction {
        SwapAction {
            venue: AmmVenue::CurveV1 {
                chain: ChainId::ethereum_mainnet(),
                pool: Address::from([1u8; 20]),
                n_coins: 2,
                is_meta: false,
            },
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: dai_ref(),
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
}
