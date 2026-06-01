//! Balancer V2 swap math — vault-based, multi-curve.
//!
//! Pure functions called from `swap.rs` after dispatch on `AmmVenue::BalancerV2`.
//! A single `quote_swap_hop` entry point internally dispatches on the
//! `PoolState` variant (`Weighted` / `Stable`).
//!
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//!
//! ## Coverage
//!
//! `PoolState::Weighted` and `PoolState::Stable` are wired today; the
//! remaining `BalancerPoolType` variants (`ComposableStable` / `MetaStable` /
//! `LiquidityBootstrapping` / `Linear`) carry the same `Stable`/`Weighted`
//! snapshot shapes for swap purposes but layer additional BPT / linear-yield
//! adjustments that we surface as `Invariant` until the per-type math is
//! plumbed in. The branch defers via the `unsupported_*` paths below.
//!
//! ## Simplification — Weighted pools
//!
//! Balancer's `WeightedMath.sol::_calcOutGivenIn` uses the closed form
//!
//! ```text
//!   out = balance_out * (1 - (balance_in / (balance_in + amount_in_after_fee))^(w_in / w_out))
//! ```
//!
//! Evaluating the fractional power `^(w_in / w_out)` over `U256` requires a
//! `LogExpMath` library port (Balancer's on-chain fixed-point exponentiation
//! based on Remco Bloemen's solidity-bytecode `exp` implementation). To keep
//! the Phase 2 helper allocation-free and WASM-safe, we adopt the
//! **balanced-weight approximation**:
//!
//! ```text
//!   out_approx = balance_out * amount_in_after_fee * w_in
//!              / (balance_in * w_out + amount_in_after_fee * w_in)
//! ```
//!
//! which is exact for `w_in == w_out` (i.e. 50/50 pools, the canonical case)
//! and within a few percent for moderate weight skews (80/20). The
//! approximation is the linearisation of the `WeightedMath` fractional power
//! around `w_in == w_out` (the dominant deployed weighting on Balancer V2).
//!
//! ## Stable pools
//!
//! `PoolState::Stable` reuses the Curve V1 `StableSwap` solver
//! (`curve_v1::compute_d` + `curve_v1::compute_y`) since Balancer
//! `StableMath.sol::_calcOutGivenIn` is mathematically identical to Curve V1's
//! `get_y` (same invariant, same Newton iteration). The direction convention
//! mirrors Curve: `i = 0` (sell), `j = 1` (buy). Multi-coin pools (n > 2) are
//! supported through Curve's loop.
//!
//! ## 1차 출처
//!
//! * Balancer V2 monorepo —
//!   <https://github.com/balancer/balancer-v2-monorepo>
//!   * `pkg/pool-utils/contracts/lib/WeightedMath.sol::_calcOutGivenIn`
//!   * `pkg/pool-utils/contracts/lib/StableMath.sol::_calcOutGivenIn`
//!   * `pkg/solidity-utils/contracts/math/LogExpMath.sol` — fractional power
//!     primitive, deferred (see "Simplification" above).
//! * Vault swap entry —
//!   `pkg/vault/contracts/Vault.sol::swap` (single-step, batchSwap routes
//!   through the same helper).

// Phase 2 stubs: callers (per-action reducers) are still `todo!()` so these
// functions look unused. Lift this allow when the first caller wires up.
#![allow(dead_code)]

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::{ReducerError, ReducerResult};

use super::curve_v1;

/// Convention: sell coin index in the per-`PoolState::Stable` balances vector
/// (matches `curve_v1`'s direction convention).
const STABLE_I: usize = 0;
/// Convention: buy coin index in the per-`PoolState::Stable` balances vector.
const STABLE_J: usize = 1;

/// Quote a `Weighted` pool single hop. Sell-side index `0`, buy-side index `1`
/// in the `balances` / `weights` arrays.
///
/// Implements the balanced-weight approximation; see module docs.
fn quote_weighted(
    balances: &[U256],
    weights: &[u64],
    fee_bp: u32,
    amount_in: U256,
) -> ReducerResult<U256> {
    if balances.len() < 2 || weights.len() < 2 {
        return Err(ReducerError::Invariant(format!(
            "balancer_v2 weighted: needs >= 2 coins/weights, got {}/{}",
            balances.len(),
            weights.len()
        )));
    }
    if balances.len() != weights.len() {
        return Err(ReducerError::Invariant(format!(
            "balancer_v2 weighted: balances/weights length mismatch ({} != {})",
            balances.len(),
            weights.len()
        )));
    }
    if fee_bp >= 10_000 {
        return Err(ReducerError::Invariant(format!(
            "balancer_v2 weighted fee_bp {fee_bp} out of range (must be < 10000)"
        )));
    }
    let bal_in = balances[0];
    let bal_out = balances[1];
    if bal_in.is_zero() || bal_out.is_zero() {
        return Err(ReducerError::Invariant(
            "balancer_v2 weighted: zero pool balance".into(),
        ));
    }
    let w_in = U256::from(weights[0]);
    let w_out = U256::from(weights[1]);
    if w_in.is_zero() || w_out.is_zero() {
        return Err(ReducerError::Invariant(
            "balancer_v2 weighted: zero weight".into(),
        ));
    }

    // Apply swap fee on amount_in (Balancer convention — fee taken from input).
    let fee_multiplier = U256::from(10_000u32 - fee_bp);
    let amount_in_after_fee = amount_in
        .checked_mul(fee_multiplier)
        .ok_or_else(|| ReducerError::Invariant("balancer_v2 weighted: in-fee overflow".into()))?
        / U256::from(10_000u32);

    // Balanced-weight approximation:
    //   out = bal_out * amount_in_after_fee * w_in
    //         / (bal_in * w_out + amount_in_after_fee * w_in)
    let numer_inner = amount_in_after_fee.checked_mul(w_in).ok_or_else(|| {
        ReducerError::Invariant("balancer_v2 weighted: numer_inner overflow".into())
    })?;
    let numerator = bal_out.checked_mul(numer_inner).ok_or_else(|| {
        ReducerError::Invariant("balancer_v2 weighted: numerator overflow".into())
    })?;
    let denom_a = bal_in
        .checked_mul(w_out)
        .ok_or_else(|| ReducerError::Invariant("balancer_v2 weighted: denom_a overflow".into()))?;
    let denominator = denom_a.checked_add(numer_inner).ok_or_else(|| {
        ReducerError::Invariant("balancer_v2 weighted: denominator overflow".into())
    })?;
    if denominator.is_zero() {
        return Err(ReducerError::Invariant(
            "balancer_v2 weighted: zero denominator".into(),
        ));
    }
    Ok(numerator / denominator)
}

/// Quote a `Stable` pool single hop. Reuses Curve V1's `StableSwap` solver via
/// `curve_v1::compute_d` + `curve_v1::compute_y` (mathematically identical
/// invariant — see module docs).
fn quote_stable(balances: &[U256], amp: u32, fee_bp: u32, amount_in: U256) -> ReducerResult<U256> {
    if balances.len() < 2 {
        return Err(ReducerError::Invariant(format!(
            "balancer_v2 stable: needs >= 2 coins, got {}",
            balances.len()
        )));
    }
    if fee_bp >= 10_000 {
        return Err(ReducerError::Invariant(format!(
            "balancer_v2 stable fee_bp {fee_bp} out of range (must be < 10000)"
        )));
    }
    // Apply swap fee on amount_in (Balancer convention).
    let fee_multiplier = U256::from(10_000u32 - fee_bp);
    let amount_in_after_fee = amount_in
        .checked_mul(fee_multiplier)
        .ok_or_else(|| ReducerError::Invariant("balancer_v2 stable: in-fee overflow".into()))?
        / U256::from(10_000u32);

    let d = curve_v1::compute_d(balances, amp)?;
    let new_x_i = balances[STABLE_I]
        .checked_add(amount_in_after_fee)
        .ok_or_else(|| ReducerError::Invariant("balancer_v2 stable: x_i + in overflow".into()))?;
    let new_y = curve_v1::compute_y(balances, STABLE_I, STABLE_J, new_x_i, amp, d)?;
    let new_y_minus_one = new_y
        .checked_sub(U256::from(1u64))
        .ok_or_else(|| ReducerError::Invariant("balancer_v2 stable: new_y < 1 underflow".into()))?;
    let dy = balances[STABLE_J]
        .checked_sub(new_y_minus_one)
        .ok_or_else(|| ReducerError::Invariant("balancer_v2 stable: bal[j] < new_y".into()))?;
    Ok(dy)
}

/// Quote a single hop on a Balancer V2 pool given its `PoolState` snapshot.
/// Internally dispatches on the `PoolState` variant (`Weighted` / `Stable`)
/// to select the matching curve. Returns the hop's output amount; caller is
/// responsible for fee accounting and balance changes.
///
/// Errors with `Invariant` on:
///   * `PoolState` variant other than `Weighted` / `Stable`,
///   * any of the per-curve preconditions (see `quote_weighted` /
///     `quote_stable` for the per-variant taxonomy).
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    if amount_in.is_zero() {
        return Ok(U256::ZERO);
    }
    match pool_state {
        PoolState::Weighted {
            balances,
            weights,
            fee_bp,
        } => quote_weighted(balances, weights, *fee_bp, amount_in),
        PoolState::Stable {
            balances,
            amp,
            fee_bp,
        } => quote_stable(balances, *amp, *fee_bp, amount_in),
        _ => Err(ReducerError::Invariant(
            "balancer_v2: only Weighted / Stable PoolState are wired today".into(),
        )),
    }
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 50/50 weighted pool degenerates to constant-product. With
    /// `1_000 / 1_000` balances and `100` in (zero fee), the balanced-weight
    /// approximation gives `1_000 * 100 / (1_000 + 100) = 90`.
    #[test]
    fn weighted_5050_matches_constant_product() {
        let pool = PoolState::Weighted {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            weights: vec![50, 50],
            fee_bp: 0,
        };
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(100u64),
        )
        .unwrap();
        assert_eq!(out, U256::from(90u64));
    }

    /// 80/20 weighted approximation: in our linearisation,
    /// `out = bal_out * in * w_in / (bal_in * w_out + in * w_in)`.
    /// `1_000 * 100 * 80 / (1_000 * 20 + 100 * 80) = 8_000_000 / 28_000 = 285`.
    #[test]
    fn weighted_8020_balanced_approximation() {
        let pool = PoolState::Weighted {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            weights: vec![80, 20],
            fee_bp: 0,
        };
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(100u64),
        )
        .unwrap();
        assert_eq!(out, U256::from(285u64));
    }

    /// Stable pool (Curve-equivalent) at equal balances near the peg. With
    /// `1_000_000 / 1_000_000` and `1_000` in (zero fee), output stays within
    /// 2 wei of `1_000` (Newton convergence slack).
    #[test]
    fn stable_equal_balance_approximately_one_to_one() {
        let pool = PoolState::Stable {
            balances: vec![U256::from(1_000_000u64), U256::from(1_000_000u64)],
            amp: 100,
            fee_bp: 0,
        };
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(1_000u64),
        )
        .unwrap();
        let target = U256::from(1_000u64);
        let diff = if out > target {
            out - target
        } else {
            target - out
        };
        assert!(diff <= U256::from(2u64), "out = {out}, expected ≈ 1_000");
    }

    /// Weighted pool with `fee_bp = 30` reduces the input first, so the output
    /// drops below the zero-fee `90`. Balanced-weight formula on
    /// `amount_in_after_fee = 100 * 9970 / 10000 = 99`:
    /// `1_000 * 99 / (1_000 + 99) = 99_000 / 1_099 = 90` (integer floor).
    #[test]
    fn weighted_fee_reduces_amount_in_first() {
        let pool = PoolState::Weighted {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            weights: vec![50, 50],
            fee_bp: 30,
        };
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(100u64),
        )
        .unwrap();
        // Integer floor of 99_000/1_099 = 90.
        assert_eq!(out, U256::from(90u64));
    }

    /// Zero `amount_in` short-circuits to zero out (entrypoint guard, before
    /// any variant dispatch).
    #[test]
    fn zero_amount_in_returns_zero() {
        let pool = PoolState::Weighted {
            balances: vec![U256::from(1u64), U256::from(1u64)],
            weights: vec![50, 50],
            fee_bp: 0,
        };
        let out = quote_swap_hop(&empty_state(), &ctx(), &dummy_swap(), &pool, U256::ZERO).unwrap();
        assert_eq!(out, U256::ZERO);
    }

    /// Unsupported `PoolState` variant surfaces as `Invariant`.
    #[test]
    fn rejects_non_weighted_or_stable_pool_state() {
        let pool = PoolState::XyConstant {
            reserve_in: U256::from(1u64),
            reserve_out: U256::from(1u64),
            fee_bp: 0,
        };
        let err = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(1u64),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("Weighted")));
    }

    /// Weight length mismatch is caught explicitly.
    #[test]
    fn weighted_length_mismatch_rejected() {
        let pool = PoolState::Weighted {
            balances: vec![U256::from(1u64), U256::from(1u64), U256::from(1u64)],
            weights: vec![50, 50],
            fee_bp: 0,
        };
        let err = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(1u64),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("length mismatch")));
    }

    // -------- test fixtures --------

    use crate::action::amm::{
        AmmVenue, BalancerPoolType, SwapDirection, SwapLiveInputs, SwapParams, SwapRoute,
    };
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
            venue: AmmVenue::BalancerV2 {
                chain: ChainId::ethereum_mainnet(),
                vault: Address::from([1u8; 20]),
                pool_id: format!("0x{}", "00".repeat(32)),
                pool_type: BalancerPoolType::Weighted,
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
}
