//! Uniswap V4 swap math — concentrated liquidity behind the singleton
//! `PoolManager` with an optional hooks contract.
//!
//! Pure functions called from `swap.rs` after dispatch on
//! `AmmVenue::UniswapV4`. Not a `Reducer` impl since `AmmVenue` is not an
//! `Action`.
//!
//! ## Phase 2F scope — **hooks-free, V3-shape math**
//!
//! Today's wiring covers only the `hooks == Address::ZERO` case. Behind the
//! singleton, V4's swap math is the same Uniswap-V3 closed form on the
//! pool's `(sqrt_price_x96, liquidity, ticks)` snapshot — the V4 spec
//! literally invokes the same `SwapMath` library that V3 uses. We therefore
//! delegate to [`super::uniswap_v3::concentrated_swap_math`] with a
//! `"uniswap_v4"` protocol tag so error messages stay attributable.
//!
//! ### Hooks dispatch (deferred)
//!
//! When `PoolKey.hooks != 0`, the pool may override the swap curve, fees,
//! or accounting via `beforeSwap` / `afterSwap` callbacks. Resolving an
//! arbitrary hook against a known-hook registry (and running its
//! mini-state machine) is out of scope for Phase 2F. The caller in
//! `swap.rs` therefore short-circuits non-zero-hooks pools with
//! `UnsupportedProtocol { protocol: "uniswap_v4_with_hooks" }`. This
//! function itself does **not** inspect hooks — its `PoolState` snapshot
//! does not carry the `PoolKey`.
//!
//! ## References
//!
//! * `PoolManager.sol`  — <https://github.com/Uniswap/v4-core/blob/main/src/PoolManager.sol>
//! * V4 swap math reuses V3's `SwapMath` / `SqrtPriceMath` libraries — see
//!   the `uniswap_v3` module's reference links.

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, WalletState};

use super::uniswap_v3;
use crate::action::amm::{PoolState, SwapAction};
use crate::error::ReducerResult;

/// Quote a single hop on a Uniswap V4 pool given its `Concentrated`
/// `PoolState` snapshot. Returns the hop's output amount; caller is
/// responsible for balance changes **and** for refusing pools whose
/// `hooks` address is non-zero (see module docs — hooks dispatch is
/// deferred and the caller short-circuits before reaching this function).
///
/// The math is identical to `uniswap_v3::quote_swap_hop`: Phase 2E's
/// simplified active-tick closed form in the `zeroForOne` direction, no
/// fee subtraction, no tick crossing. See the `uniswap_v3` module docs
/// for the algebra and the caveats.
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    uniswap_v3::concentrated_swap_math(pool_state, amount_in, "uniswap_v4")
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{AmmVenue, SwapDirection, SwapLiveInputs, SwapParams, SwapRoute};
    use crate::error::ReducerError;
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

    /// Mainnet `PoolManager` address is irrelevant to the math; we just
    /// need a non-zero placeholder for the venue field.
    fn v4_venue_no_hooks() -> AmmVenue {
        AmmVenue::UniswapV4 {
            chain: ChainId::ethereum_mainnet(),
            pool_id: "0x0000000000000000000000000000000000000000000000000000000000000001".into(),
            pool_manager: Address::from([2u8; 20]),
            hooks: Address::ZERO,
        }
    }

    fn dummy_swap() -> SwapAction {
        SwapAction {
            venue: v4_venue_no_hooks(),
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
    /// `sqrt_price_x96`, empty `ticks` snapshot.
    fn pool(liquidity: u128, sqrt_price_x96: U256) -> PoolState {
        PoolState::Concentrated {
            sqrt_price_x96,
            tick: 0,
            liquidity: U128::from(liquidity),
            ticks: vec![],
        }
    }

    /// `Q96 = 2^96`.
    fn q96_val() -> U256 {
        U256::from(1u64) << 96
    }

    // ----------------------------------------------------------------------
    // V4 hooks-free path delegates to the same closed form as V3:
    //   L = 1_000_000, a = 1_000, sp = Q96  →  out = L*a/(L+a) = 999.
    // ----------------------------------------------------------------------

    #[test]
    fn quote_matches_v3_closed_form_for_hooks_zero_pool() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(1_000_000, q96_val());
        let out = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1_000u64)).unwrap();
        assert_eq!(out, U256::from(999u64));
    }

    // ----------------------------------------------------------------------
    // Zero liquidity surfaces as Invariant with the V4 protocol tag.
    // ----------------------------------------------------------------------

    #[test]
    fn quote_zero_liquidity_errors_with_v4_tag() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = pool(0, q96_val());
        let err = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1u64)).unwrap_err();
        let ReducerError::Invariant(msg) = err else {
            panic!("expected Invariant, got something else");
        };
        assert!(msg.contains("zero active liquidity"));
        assert!(msg.contains("uniswap_v4"));
    }

    // ----------------------------------------------------------------------
    // Non-Concentrated pool variant rejected with V4-tagged message.
    // ----------------------------------------------------------------------

    #[test]
    fn quote_rejects_non_concentrated_pool_state_with_v4_tag() {
        let state = empty_state();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(10_000u64),
            reserve_out: U256::from(10_000u64),
            fee_bp: 30,
        };
        let err = quote_swap_hop(&state, &ctx(), &swap, &pool_state, U256::from(1u64)).unwrap_err();
        let ReducerError::Invariant(msg) = err else {
            panic!("expected Invariant");
        };
        assert!(msg.contains("Concentrated"));
        assert!(msg.contains("uniswap_v4"));
    }
}
