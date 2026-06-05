//! `SushiSwap V2` swap math.
//! Math identical to `uniswap_v2`; kept separate for explicit venue dispatch
//! on `AmmVenue::SushiV2` so per-venue fee schedules and pool registries can
//! diverge later without touching the V2 file.
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//! * `SushiSwap V2` contracts (`Uniswap V2` fork) —
//!   <https://github.com/sushiswap/sushiswap/tree/master/protocols/sushiswap>
//!   * `UniswapV2Pair.sol::getAmountOut` — identical `x*y=k` formula.
//!   * Default fee 30 bp, expressed as the `(10000 - fee_bp)` multiplier — same
//!     wire convention this module's `quote_swap_hop` consumes.

use policy_state::primitives::U256;
use policy_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::ReducerResult;

use super::uniswap_v2;

/// Quote a single hop on a `SushiSwap V2` pool given its `XyConstant`
/// `PoolState` snapshot. Returns the hop's output amount; caller is responsible
/// for fee accounting and balance changes.
/// `SushiSwap V2` is a direct `Uniswap V2` fork (`UniswapV2Pair.sol`) with
/// identical `getAmountOut` math, so this is a thin proxy to
/// [`uniswap_v2::quote_swap_hop`]. The dedicated entry point exists so
/// per-venue customisation (e.g. trident-style hybrid pools, BentoBox-routed
/// reserves) can diverge later without touching the V2 helper.
/// Errors are produced verbatim by [`uniswap_v2::quote_swap_hop`] — see that
/// helper for the full failure-mode taxonomy.
pub(super) fn quote_swap_hop(
    state: &WalletState,
    ctx: &EvalContext,
    swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    uniswap_v2::quote_swap_hop(state, ctx, swap, pool_state, amount_in)
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
            venue: AmmVenue::SushiV2 {
                chain: ChainId::ethereum_mainnet(),
                pool: Address::from([1u8; 20]),
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

    /// Sushi proxy must produce byte-for-byte the same output as the V2 helper
    /// on the canonical `getAmountOut` fixture: `1_000` in, `10_000 / 10_000`
    /// reserves, 30 bp fee → `906` out.
    #[test]
    fn sushi_v2_proxy_matches_uniswap_v2_canonical() {
        let state = empty_state();
        let ctx = ctx();
        let swap = dummy_swap();
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(10_000u64),
            reserve_out: U256::from(10_000u64),
            fee_bp: 30,
        };
        let sushi_out =
            quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(1_000u64)).unwrap();
        let uni_out =
            uniswap_v2::quote_swap_hop(&state, &ctx, &swap, &pool_state, U256::from(1_000u64))
                .unwrap();
        assert_eq!(sushi_out, uni_out);
        assert_eq!(sushi_out, U256::from(906u64));
    }
}
