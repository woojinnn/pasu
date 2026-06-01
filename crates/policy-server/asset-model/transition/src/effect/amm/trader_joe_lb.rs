//! Trader Joe Liquidity Book swap math — discrete bins with variable fee.
//!
//! Pure functions called from `swap.rs` after dispatch on
//! `AmmVenue::TraderJoeLB`. Not a `Reducer` impl since `AmmVenue` is not an
//! `Action`.
//!
//! ## Simplification — **single active-bin closed form**
//!
//! Trader Joe LB partitions liquidity into discrete bins at fixed price steps
//! (`bin_step` basis points). A full swap walks through bins in price order,
//! consuming each bin's reserves until `amount_in` is exhausted (analogous to
//! Uniswap V3 tick traversal but on discrete price levels). The on-chain
//! implementation lives in `LBPair.sol::swap` and `BinHelper.sol::getAmounts`.
//!
//! This helper instead evaluates the swap **only on the active bin**, treating
//! that bin as a local constant-product pool with the `variable_fee_bp`
//! applied to `amount_in`. The simplification is accurate when:
//!   * the trade is small enough to stay within the active bin's reserves, or
//!   * the user-facing accounting only needs an order-of-magnitude quote.
//!
//! Bin walking is bundled with the follow-up tick-traversal work tracked for
//! Uniswap V3 (see `uniswap_v3` module docs); the two share the same
//! "step through liquidity until exhausted" shape.
//!
//! ## 1차 출처
//!
//! * Trader Joe Liquidity Book whitepaper —
//!   <https://docs.traderjoexyz.com/concepts/liquidity-book>
//! * Joe-v2 contracts —
//!   <https://github.com/traderjoe-xyz/joe-v2/blob/main/src/LBPair.sol>
//!   * `swap` — entry point; loops `BinHelper.getAmounts` per bin.
//!   * `getAmounts` (in `BinHelper.sol`) — per-bin closed form; the basis
//!     for this helper's active-bin quote.
//! * Variable surge fee —
//!   <https://github.com/traderjoe-xyz/joe-v2/blob/main/src/libraries/FeeHelper.sol>
//!   `getVariableFee` (volatility-adjusted, expressed in `1e18` units in the
//!   contract; this helper consumes the already-derived `variable_fee_bp`
//!   carried on `PoolState::LiquidityBook`).

// Phase 2 stubs: callers (per-action reducers) are still `todo!()` so these
// functions look unused. Lift this allow when the first caller wires up.
#![allow(dead_code)]

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::{ReducerError, ReducerResult};

/// Quote a single hop on a Trader Joe Liquidity Book pool given its
/// `LiquidityBook` `PoolState` snapshot. Returns the hop's output amount;
/// caller is responsible for balance changes (the variable surge fee is
/// already applied here).
///
/// **Simplification** — single active-bin closed form; see module docs.
///
/// Errors with `Invariant` on:
///   * `PoolState` variant other than `LiquidityBook`,
///   * `bins` empty,
///   * `active_bin_id` not found in `bins`,
///   * `variable_fee_bp >= 10_000`,
///   * zero or empty active-bin reserves on the sell side,
///   * any `U256` overflow.
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    let PoolState::LiquidityBook {
        active_bin_id,
        bins,
        variable_fee_bp,
    } = pool_state
    else {
        return Err(ReducerError::Invariant(
            "non-LiquidityBook pool_state for trader_joe_lb swap".into(),
        ));
    };
    if bins.is_empty() {
        return Err(ReducerError::Invariant("trader_joe_lb: empty bins".into()));
    }
    if *variable_fee_bp >= 10_000 {
        return Err(ReducerError::Invariant(format!(
            "trader_joe_lb variable_fee_bp {variable_fee_bp} out of range (must be < 10000)"
        )));
    }
    if amount_in.is_zero() {
        return Ok(U256::ZERO);
    }

    let active = bins
        .iter()
        .find(|b| b.id == *active_bin_id)
        .ok_or_else(|| {
            ReducerError::Invariant(format!(
                "trader_joe_lb: active_bin_id {active_bin_id} not in bins"
            ))
        })?;
    if active.reserve_in.is_zero() || active.reserve_out.is_zero() {
        return Err(ReducerError::Invariant(
            "trader_joe_lb: zero active-bin reserve".into(),
        ));
    }

    // Apply variable fee on amount_in (Trader Joe LB convention — fee taken
    // from the input side at each bin step).
    let fee_multiplier = U256::from(10_000u32 - *variable_fee_bp);
    let amount_in_after_fee = amount_in
        .checked_mul(fee_multiplier)
        .ok_or_else(|| ReducerError::Invariant("trader_joe_lb: in-fee overflow".into()))?
        / U256::from(10_000u32);

    // Active-bin constant-product:
    //   out = reserve_out * in_after_fee / (reserve_in + in_after_fee)
    let numerator = active
        .reserve_out
        .checked_mul(amount_in_after_fee)
        .ok_or_else(|| ReducerError::Invariant("trader_joe_lb: numerator overflow".into()))?;
    let denominator = active
        .reserve_in
        .checked_add(amount_in_after_fee)
        .ok_or_else(|| ReducerError::Invariant("trader_joe_lb: denominator overflow".into()))?;
    if denominator.is_zero() {
        return Err(ReducerError::Invariant(
            "trader_joe_lb: zero denominator".into(),
        ));
    }
    Ok(numerator / denominator)
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::BinState;

    /// Active bin with `1_000 / 1_000` reserves, zero fee, `100` in →
    /// `1_000 * 100 / 1_100 = 90`.
    #[test]
    fn active_bin_quote_matches_constant_product() {
        let pool = PoolState::LiquidityBook {
            active_bin_id: 7,
            bins: vec![BinState {
                id: 7,
                reserve_in: U256::from(1_000u64),
                reserve_out: U256::from(1_000u64),
            }],
            variable_fee_bp: 0,
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

    /// Variable fee subtracts from `amount_in` first. `30 bp` on `100` in
    /// gives `99` after fee; `1_000 * 99 / 1_099 = 90`.
    #[test]
    fn variable_fee_applied_on_amount_in() {
        let pool = PoolState::LiquidityBook {
            active_bin_id: 7,
            bins: vec![BinState {
                id: 7,
                reserve_in: U256::from(1_000u64),
                reserve_out: U256::from(1_000u64),
            }],
            variable_fee_bp: 30,
        };
        let out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(100u64),
        )
        .unwrap();
        // 99_000 / 1_099 = 90 (integer floor).
        assert_eq!(out, U256::from(90u64));
    }

    /// Inactive-bin only fixture: the `active_bin_id` is not in the bins
    /// list.
    /// Surfaces as `Invariant` so the policy / observability layer can see it.
    #[test]
    fn missing_active_bin_id_errors() {
        let pool = PoolState::LiquidityBook {
            active_bin_id: 7,
            bins: vec![BinState {
                id: 8,
                reserve_in: U256::from(1u64),
                reserve_out: U256::from(1u64),
            }],
            variable_fee_bp: 0,
        };
        let err = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(1u64),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("active_bin_id")));
    }

    /// Empty bins list rejected.
    #[test]
    fn empty_bins_errors() {
        let pool = PoolState::LiquidityBook {
            active_bin_id: 7,
            bins: vec![],
            variable_fee_bp: 0,
        };
        let err = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(1u64),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("empty bins")));
    }

    /// Zero `amount_in` short-circuits to zero out.
    #[test]
    fn zero_amount_in_returns_zero() {
        let pool = PoolState::LiquidityBook {
            active_bin_id: 7,
            bins: vec![BinState {
                id: 7,
                reserve_in: U256::from(1u64),
                reserve_out: U256::from(1u64),
            }],
            variable_fee_bp: 0,
        };
        let out = quote_swap_hop(&empty_state(), &ctx(), &dummy_swap(), &pool, U256::ZERO).unwrap();
        assert_eq!(out, U256::ZERO);
    }

    /// Non-`LiquidityBook` variants rejected.
    #[test]
    fn rejects_non_liquidity_book_pool_state() {
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
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("LiquidityBook")));
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
            venue: AmmVenue::TraderJoeLB {
                chain: ChainId::ethereum_mainnet(),
                pair: Address::from([1u8; 20]),
                bin_step: 10,
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
