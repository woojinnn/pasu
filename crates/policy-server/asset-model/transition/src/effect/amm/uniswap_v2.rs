//! Uniswap V2 swap math — `x * y = k` constant product.
//! Pure functions called from `swap.rs` after dispatch on `AmmVenue::UniswapV2`.
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//! ## Fee convention
//! `PoolState::XyConstant.fee_bp` is in **basis points** (1 bp = 0.01%); the
//! canonical Uniswap V2 spec fee of `0.3 %` therefore corresponds to
//! `fee_bp = 30`. Matches the `fee_bp` units already used by
//! `PoolState::Cryptoswap` and the `effective_fee_bp` field on `RouteHop`
//! (e.g. fixture #5 uses `5` for the V3 `0.05 %` tier).
//! The V3 enum field `fee_tier_bp` *does* use the `bp × 100` convention but
//! that variant is `Concentrated`, not `XyConstant`; only the latter is
//! consumed by this module.

use policy_state::primitives::U256;
use policy_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::{ReducerError, ReducerResult};

/// Quote a single hop on a Uniswap V2 pool given its `XyConstant` `PoolState`
/// snapshot. Returns the hop's output amount; caller is responsible for fee
/// accounting and balance changes.
/// Implements the canonical `getAmountOut` formula:
/// ```text
///   amount_in_with_fee = amount_in * (10_000 - fee_bp)
///   numerator          = reserve_out * amount_in_with_fee
///   denominator        = reserve_in  * 10_000 + amount_in_with_fee
///   amount_out         = numerator / denominator
/// ```
/// Returns `Invariant` on:
///   * `fee_bp >= 10_000` (would zero / underflow the fee multiplier),
///   * any `U256` overflow during the multiplications,
///   * zero denominator (empty pool — `reserve_in == 0` and `amount_in == 0`),
///   * `PoolState` variant other than `XyConstant`.
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    match pool_state {
        PoolState::XyConstant {
            reserve_in,
            reserve_out,
            fee_bp,
        } => {
            if *fee_bp >= 10_000 {
                return Err(ReducerError::Invariant(format!(
                    "uniswap_v2 fee_bp {fee_bp} out of range (must be < 10000)"
                )));
            }
            let fee_multiplier = U256::from(10_000u32 - *fee_bp);
            let amount_in_with_fee = amount_in.checked_mul(fee_multiplier).ok_or_else(|| {
                ReducerError::Invariant("uniswap_v2 amount_in_with_fee overflow".into())
            })?;
            let numerator = reserve_out
                .checked_mul(amount_in_with_fee)
                .ok_or_else(|| ReducerError::Invariant("uniswap_v2 numerator overflow".into()))?;
            let denom_base = reserve_in
                .checked_mul(U256::from(10_000u32))
                .ok_or_else(|| {
                    ReducerError::Invariant("uniswap_v2 denominator base overflow".into())
                })?;
            let denominator = denom_base
                .checked_add(amount_in_with_fee)
                .ok_or_else(|| ReducerError::Invariant("uniswap_v2 denominator overflow".into()))?;
            if denominator.is_zero() {
                return Err(ReducerError::Invariant(
                    "uniswap_v2 zero denominator (empty pool)".into(),
                ));
            }
            Ok(numerator / denominator)
        }
        _ => Err(ReducerError::Invariant(
            "non-XyConstant pool_state for uniswap_v2 swap".into(),
        )),
    }
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{AmmVenue, SwapDirection, SwapLiveInputs, SwapParams, SwapRoute};
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::wallet::WalletId;
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
            venue: AmmVenue::UniswapV2 {
                chain: ChainId::ethereum_mainnet(),
                pool: Address::from([1u8; 20]),
                factory: Address::from([2u8; 20]),
            },
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: Some(weth_ref()),
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

    /// Canonical `getAmountOut` cross-check against a closed-form value.
    /// `amount_in = 1_000`, `reserve_in = 10_000`, `reserve_out = 10_000`,
    /// `fee_bp = 30`:
    ///   `amount_in_with_fee = 1000 * 9970 = 9_970_000`
    ///   `numerator          = 10_000 * 9_970_000 = 99_700_000_000`
    ///   `denominator        = 10_000 * 10_000 + 9_970_000 = 109_970_000`
    ///   `out                = 99_700_000_000 / 109_970_000 = 906`
    #[test]
    fn quote_xy_constant_canonical_formula() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(10_000u64),
            reserve_out: U256::from(10_000u64),
            fee_bp: 30,
        };
        let out = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(1_000u64)).unwrap();
        assert_eq!(out, U256::from(906u64));
    }

    /// Zero-fee pool: `amount_in_with_fee = amount_in * 10_000`.
    /// `1 * 10_000 / (1 * 10_000 + 1 * 10_000)` against equal reserves with
    /// `amount_in = 1` and `reserves = 100` → `out = 100 * 1 * 10_000 /
    /// (100 * 10_000 + 1 * 10_000) = 1_000_000 / 1_010_000 = 0` (integer).
    /// With larger `amount_in = 50`: `100 * 50 * 10_000 / (100 * 10_000 +
    /// 50 * 10_000) = 50_000_000 / 1_500_000 = 33`.
    #[test]
    fn quote_xy_constant_zero_fee() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(100u64),
            reserve_out: U256::from(100u64),
            fee_bp: 0,
        };
        let out = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(50u64)).unwrap();
        assert_eq!(out, U256::from(33u64));
    }

    /// Zero `amount_in` produces zero `amount_out` (no underflow).
    #[test]
    fn quote_xy_constant_zero_amount_in() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(1_000u64),
            reserve_out: U256::from(1_000u64),
            fee_bp: 30,
        };
        let out = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::ZERO).unwrap();
        assert_eq!(out, U256::ZERO);
    }

    /// `fee_bp >= 10_000` (impossible 100 %+ fee) is an invariant error.
    #[test]
    fn quote_xy_constant_oversized_fee_errors() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(1_000u64),
            reserve_out: U256::from(1_000u64),
            fee_bp: 10_000,
        };
        let err = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(10u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("fee_bp")));
    }

    /// Empty pool (`reserve_in == 0` and `amount_in == 0`) yields zero
    /// denominator → invariant error.
    #[test]
    fn quote_xy_constant_empty_pool_zero_in_errors() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::ZERO,
            reserve_out: U256::from(1_000u64),
            fee_bp: 30,
        };
        let err = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::ZERO).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("zero denominator")));
    }

    /// Non-`XyConstant` variants must be rejected.
    #[test]
    fn quote_rejects_non_xy_constant_pool_state() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: policy_state::primitives::U128::from(0u64),
            ticks: vec![],
        };
        let err = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("XyConstant")));
    }

    /// Sanity / no-fee monotonicity: doubling `amount_in` increases `amount_out`.
    #[test]
    fn quote_xy_constant_monotonic_in_amount_in() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(1_000_000u64),
            reserve_out: U256::from(1_000_000u64),
            fee_bp: 30,
        };
        let small = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(1_000u64)).unwrap();
        let large = quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(2_000u64)).unwrap();
        assert!(large > small);
    }
}
