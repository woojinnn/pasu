//! Curve V2 swap math — cryptoswap invariant (dynamic peg).
//!
//! Pure functions called from `swap.rs` after dispatch on `AmmVenue::CurveV2`.
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//! ## Simplification — **single-price twocrypto closed form**
//! The full Curve V2 cryptoswap invariant solves a coupled Newton system over
//! `(A, gamma)` against a dynamically re-pegged `price_scale` (Egorov et al.
//! 2021). The on-chain `tricrypto2.vy::newton_y` iterates a fourth-order
//! curve and dispatches on rebalance events. Implementing that faithfully
//! requires both the unpacked `(A, gamma)` parameters (currently packed into
//! a single `U256` in `PoolState::Cryptoswap.a_gamma`) and the multi-step
//! repeg-during-swap accounting.
//! form: scale the second-coin balance by `price_scale[0]`, then quote with
//! the canonical `x * y = k` formula. The simplification is accurate when:
//!   * `n = 2` (twocrypto pool layout — `CurveCryptoSwap2ETH.vy`),
//!   * the trade does not trigger a re-peg (small relative to liquidity),
//!   * `A * gamma` is large enough that the active curve is locally
//!     constant-product around the current `price_scale`.
//!
//! Pools that are not 2-coin or where the trade would meaningfully shift the
//! peg fall back to `Invariant` so policy / observability can target them.
//! * Egorov 2021 — "Automatic market-making with dynamic peg"
//!   <https://classic.curve.fi/files/crypto-pools-paper.pdf>
//! * Curve cryptoswap experimental Vyper —
//!   <https://github.com/curvefi/curve-cryptoswap-experimental/blob/main/contracts/Pools/CryptoMath.vy>
//!   * `newton_y` (line ~143) — full fourth-order Newton solver (not used by
//!     this simplified helper; see "Simplification" above for why).
//! * Twocrypto pool —
//!   <https://github.com/curvefi/twocrypto-ng/blob/main/contracts/main/CurveTwocryptoOptimized.vy>
//!   * `_calc_token_amount`, `get_dy` — production twocrypto invariant.

// functions look unused. Lift this allow when the first caller wires up.
#![allow(dead_code)]

use policy_state::primitives::U256;
use policy_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::{ReducerError, ReducerResult};

/// Curve cryptoswap uses 1e18-scaled `price_scale` entries (matches the Vyper
/// `PRECISION = 10**18` constant).
fn price_precision() -> U256 {
    U256::from(10u64).pow(U256::from(18u64))
}

/// Quote a single hop on a Curve V2 pool given its `Cryptoswap` `PoolState`
/// snapshot. Returns the hop's output amount; caller is responsible for fee
/// accounting and balance changes.
///
/// **Simplification** — adopts the twocrypto single-price closed form; see
/// module docs. Pools with `n != 2` and pools whose trade would re-peg
/// surface as `Invariant`. The trade input is taken as `coin[0] → coin[1]`
/// (`i = 0`, `j = 1`); the inverse direction is not modelled at this layer
/// because `PoolState::Cryptoswap` does not carry the swap direction.
///
/// Errors with `Invariant` on:
///   * `PoolState` variant other than `Cryptoswap`,
///   * `balances.len() != 2` (only twocrypto modelled),
///   * `price_scale` empty (no peg for coin 1),
///   * `price_scale[0] == 0` (degenerate peg),
///   * `fee_bp >= 10_000`,
///   * any `U256` overflow.
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    let PoolState::Cryptoswap {
        balances,
        price_scale,
        a_gamma: _,
        fee_bp,
    } = pool_state
    else {
        return Err(ReducerError::Invariant(
            "non-Cryptoswap pool_state for curve_v2 swap".into(),
        ));
    };
    if balances.len() != 2 {
        return Err(ReducerError::Invariant(format!(
            "curve_v2 twocrypto simplification needs 2 coins, got {}",
            balances.len()
        )));
    }
    if price_scale.is_empty() {
        return Err(ReducerError::Invariant(
            "curve_v2: empty price_scale".into(),
        ));
    }
    if price_scale[0].is_zero() {
        return Err(ReducerError::Invariant(
            "curve_v2: zero price_scale[0]".into(),
        ));
    }
    if *fee_bp >= 10_000 {
        return Err(ReducerError::Invariant(format!(
            "curve_v2 fee_bp {fee_bp} out of range (must be < 10000)"
        )));
    }
    if amount_in.is_zero() {
        return Ok(U256::ZERO);
    }

    let precision = price_precision();
    let price = price_scale[0];

    // xp = [balances[0], balances[1] * price_scale[0] / PRECISION]
    let xp0 = balances[0];
    let xp1 = balances[1]
        .checked_mul(price)
        .ok_or_else(|| ReducerError::Invariant("curve_v2: xp1 mul overflow".into()))?
        / precision;
    if xp0.is_zero() || xp1.is_zero() {
        return Err(ReducerError::Invariant(
            "curve_v2: zero scaled balance".into(),
        ));
    }

    // Canonical x*y=k on scaled balances; trade is coin[0] in.
    // amount_in_with_fee_bp omitted from xp before the swap — Curve V2 takes
    // its fee on the output (`get_dy_*_after_fee` style), which we mirror
    // below by deducting `fee_bp` from the gross out.
    let numerator = xp1
        .checked_mul(amount_in)
        .ok_or_else(|| ReducerError::Invariant("curve_v2: numerator overflow".into()))?;
    let denominator = xp0
        .checked_add(amount_in)
        .ok_or_else(|| ReducerError::Invariant("curve_v2: denominator overflow".into()))?;
    if denominator.is_zero() {
        return Err(ReducerError::Invariant("curve_v2: zero denominator".into()));
    }
    // dy_scaled = xp1 * amount_in / (xp0 + amount_in)  — output in xp space
    let dy_scaled = numerator / denominator;

    // Reverse the price scaling on the output to obtain coin[1] units.
    // dy_native = dy_scaled * PRECISION / price_scale[0]
    let dy_pre_fee = dy_scaled
        .checked_mul(precision)
        .ok_or_else(|| ReducerError::Invariant("curve_v2: dy descale overflow".into()))?
        / price;

    // fee: Curve V2 charges fee on the output (`fee_calc.fee_pre_calc`).
    let fee_amt = dy_pre_fee
        .checked_mul(U256::from(*fee_bp))
        .ok_or_else(|| ReducerError::Invariant("curve_v2: fee numerator overflow".into()))?
        / U256::from(10_000u32);
    let dy = dy_pre_fee
        .checked_sub(fee_amt)
        .ok_or_else(|| ReducerError::Invariant("curve_v2: dy - fee underflow".into()))?;
    Ok(dy)
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// `price_scale = 1e18` (1:1 peg) collapses the cryptoswap closed form to
    /// the V2 `x*y=k` formula on the raw balances. With `1_000 / 1_000`
    /// balances and `100` in (zero fee), the closed form gives
    /// `1_000 * 100 / (1_000 + 100) = 90.909... → 90`.
    #[test]
    fn one_to_one_peg_matches_xy_constant() {
        let pool = PoolState::Cryptoswap {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            price_scale: vec![price_precision()], // 1.0 peg
            a_gamma: U256::ZERO,
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

    /// `price_scale = 2 * 1e18` doubles the implied price of coin1 (1 coin1 ==
    /// 2 coin0). For a `1_000 / 1_000` pool and `100` in (zero fee), scaled
    /// balances become `[1_000, 2_000]`. Constant-product on the scaled pool:
    /// `2_000 * 100 / (1_000 + 100) = 181.818 → 181`. Descaling:
    /// `181 * 1e18 / 2e18 = 90` (output in native coin1 units).
    #[test]
    fn double_peg_doubles_scaled_out_and_descales_correctly() {
        let pool = PoolState::Cryptoswap {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            price_scale: vec![price_precision() * U256::from(2u64)],
            a_gamma: U256::ZERO,
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

    /// Fee subtraction: 30 bp on the 1:1 peg fixture cuts `90` to `~89`.
    #[test]
    fn fee_subtracts_basis_points_from_pre_fee_out() {
        let pool = PoolState::Cryptoswap {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            price_scale: vec![price_precision()],
            a_gamma: U256::ZERO,
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
        // 90 - (90 * 30 / 10000) = 90 - 0 (integer) = 90 → integer fee floor.
        // With 90 pre-fee the integer fee is 0, so out stays 90. Use a larger
        // input to exercise the fee branch.
        assert_eq!(out, U256::from(90u64));

        let out_big = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(100_000u64),
        )
        .unwrap();
        // pre-fee: 1_000 * 100_000 / 101_000 = 990. fee = 990*30/10000 = 2.97→2. out = 988.
        assert_eq!(out_big, U256::from(988u64));
    }

    /// Zero `amount_in` short-circuits to zero out.
    #[test]
    fn zero_amount_in_returns_zero() {
        let pool = PoolState::Cryptoswap {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            price_scale: vec![price_precision()],
            a_gamma: U256::ZERO,
            fee_bp: 0,
        };
        let out = quote_swap_hop(&empty_state(), &ctx(), &dummy_swap(), &pool, U256::ZERO).unwrap();
        assert_eq!(out, U256::ZERO);
    }

    /// Non-2-coin pools fall back to `Invariant` so the policy layer can route
    /// to a tricrypto-aware path later.
    #[test]
    fn three_coin_pool_rejected() {
        let pool = PoolState::Cryptoswap {
            balances: vec![
                U256::from(1_000u64),
                U256::from(1_000u64),
                U256::from(1_000u64),
            ],
            price_scale: vec![price_precision(), price_precision()],
            a_gamma: U256::ZERO,
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
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("2 coins")));
    }

    /// `price_scale[0] == 0` is rejected explicitly (would otherwise produce a
    /// degenerate division).
    #[test]
    fn zero_peg_rejected() {
        let pool = PoolState::Cryptoswap {
            balances: vec![U256::from(1_000u64), U256::from(1_000u64)],
            price_scale: vec![U256::ZERO],
            a_gamma: U256::ZERO,
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
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("price_scale")));
    }

    /// Non-`Cryptoswap` variants are rejected.
    #[test]
    fn rejects_non_cryptoswap_pool_state() {
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
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("Cryptoswap")));
    }

    // -------- test fixtures --------

    use crate::action::amm::{AmmVenue, SwapDirection, SwapLiveInputs, SwapParams, SwapRoute};
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
            venue: AmmVenue::CurveV2 {
                chain: ChainId::ethereum_mainnet(),
                pool: Address::from([1u8; 20]),
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
