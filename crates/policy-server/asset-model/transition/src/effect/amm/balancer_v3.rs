//! Balancer V3 swap math.
//! Same pool types as V2 (`Weighted` / `Stable` / `ComposableStable` /
//! `MetaStable` / `LiquidityBootstrapping` / `Linear`), but V3 introduces a
//! hook framework, a unified router, and buffer-based ERC-4626 wrappers — the
//! V3 dispatch must therefore resolve hooks before falling through to the
//! per-pool-type math, analogous to `uniswap_v4`.
//! A single `quote_swap_hop` entry point internally dispatches on the
//! `PoolState` variant. Hooks dispatch lives upstream in `swap.rs` (mirroring
//! the V4 pattern — the hooks address is on the `AmmVenue::BalancerV3`
//! variant, not on the `PoolState`); this helper only handles hook-less pool
//! math and otherwise delegates to the shared V2 helpers since the underlying
//! `WeightedMath` / `StableMath` libraries are unchanged from V2 to V3.
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//! * Balancer V3 monorepo —
//!   <https://github.com/balancer/balancer-v3-monorepo>
//!   * `pkg/pool-weighted/contracts/WeightedPool.sol`
//!   * `pkg/pool-stable/contracts/StablePool.sol`
//!   * `pkg/vault/contracts/Vault.sol::swap`
//!   * `pkg/pool-hooks/contracts/Hooks.sol` — hooks framework (deferred,
//!     analogous to V4: see "Hooks" below).
//! ## Hooks
//! V3's hook framework can override `onSwap` / `onAfterSwap` to mutate the
//! The hooks short-circuit, when added, will live in `swap.rs` — analogous
//! to the `AmmVenue::UniswapV4 { hooks, .. }` check — because the hooks
//! address is on the `AmmVenue` discriminant, not the `PoolState`.

// functions look unused. Lift this allow when the first caller wires up.
#![allow(dead_code)]

use policy_state::primitives::U256;
use policy_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::ReducerResult;

use super::balancer_v2;

/// Quote a single hop on a Balancer V3 pool given its `PoolState` snapshot.
/// Internally delegates to [`balancer_v2::quote_swap_hop`] because the V3
/// `WeightedMath` / `StableMath` libraries are unchanged from V2; the V3
/// surface differences (hooks, buffers) live at the routing layer, not the
/// curve math.
/// Returns the hop's output amount; caller is responsible for fee accounting
/// and balance changes.
/// Errors are produced verbatim by [`balancer_v2::quote_swap_hop`] — see that
/// helper for the full failure-mode taxonomy.
pub(super) fn quote_swap_hop(
    state: &WalletState,
    ctx: &EvalContext,
    swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    balancer_v2::quote_swap_hop(state, ctx, swap, pool_state, amount_in)
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// V3 entry point delegates to V2 math — cross-check on the 50/50 weighted
    /// fixture (`1_000 / 1_000` reserves, `100` in, zero fee → 90 out).
    #[test]
    fn v3_proxy_matches_v2_weighted() {
        let pool = PoolState::Weighted {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            weights: vec![50, 50],
            fee_bp: 0,
        };
        let v3_out = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(100u64),
        )
        .unwrap();
        let v2_out = balancer_v2::quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(100u64),
        )
        .unwrap();
        assert_eq!(v3_out, v2_out);
        assert_eq!(v3_out, U256::from(90u64));
    }

    // -------- test fixtures --------

    use crate::action::amm::{
        AmmVenue, BalancerPoolType, SwapDirection, SwapLiveInputs, SwapParams, SwapRoute,
    };
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{Address, ChainId, Time};
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
            venue: AmmVenue::BalancerV3 {
                chain: ChainId::ethereum_mainnet(),
                pool_id: format!("0x{}", "00".repeat(32)),
                pool_type: BalancerPoolType::Weighted,
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
}
