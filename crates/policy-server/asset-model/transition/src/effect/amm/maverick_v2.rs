//! Maverick V2 swap math — directional (mode-aware) liquidity.
//!
//! Pure functions called from `swap.rs` after dispatch on `AmmVenue::MaverickV2`.
//! Not a `Reducer` impl since `AmmVenue` is not an `Action`.
//!
//! ## Simplification — **single active-bin closed form**
//!
//! Maverick V2 uses bin-based liquidity (analogous to Trader Joe LB) with an
//! additional **directional mode** (`Static` / `Right` / `Left` / `Both`) that
//! shifts liquidity in the direction of price movement. Full simulation
//! requires walking adjacent bins per the mode's shift rule, modelled in the
//! on-chain `Pool.sol::swap` library.
//!
//! `PoolState::Maverick` carries the active-bin payload in a free-form
//! `raw: serde_json::Value` field plus a `mode: String` discriminator. This
//! helper extracts a flat `(reserve_in, reserve_out, fee_bp)` triple from the
//! raw payload — the bin-resolved snapshot that the sync orchestrator
//! produces — and quotes against it with the canonical constant-product
//! formula. Mode-aware bin shifting is deferred (tracked alongside the
//! Uniswap V3 / Trader Joe LB tick-traversal work).
//!
//! The expected `raw` shape is:
//!
//! ```json
//! {
//!   "reserve_in":  "1000000000000000000",
//!   "reserve_out": "1000000000000000000",
//!   "fee_bp":      30
//! }
//! ```
//!
//! `mode` is currently accepted as informational only — full mode-aware
//! dispatch is deferred. Pools whose `raw` payload does not carry the
//! expected fields surface as `Invariant`.
//!
//! ## 1차 출처
//!
//! * Maverick V2 documentation —
//!   <https://docs.mav.xyz/protocol/v2>
//!   * "Directional liquidity" — `mode_left` / `mode_right` / `mode_static` /
//!     `mode_dynamic` description (mirrors the `mode` string field).
//!   * "Bin liquidity" — bin shape + reserve accounting that this helper
//!     consumes through the flattened `(reserve_in, reserve_out, fee_bp)`
//!     view.
//! * `Pool.sol` (v2) — <https://github.com/maverickprotocol/maverick-v2-contracts>
//!   `swap` library + `BinMath` for the full per-mode bin-walking math.

// Phase 2 stubs: callers (per-action reducers) are still `todo!()` so these
// functions look unused. Lift this allow when the first caller wires up.
#![allow(dead_code)]

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, WalletState};

use crate::action::amm::{PoolState, SwapAction};
use crate::error::{ReducerError, ReducerResult};

/// Pull a `U256` string field out of a `serde_json::Value` object.
fn pull_u256(raw: &serde_json::Value, key: &str) -> ReducerResult<U256> {
    let s = raw
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ReducerError::Invariant(format!(
                "maverick_v2 raw payload: missing or non-string `{key}`"
            ))
        })?;
    s.parse::<U256>().map_err(|e| {
        ReducerError::Invariant(format!("maverick_v2 raw payload: bad `{key}` U256: {e}"))
    })
}

/// Pull a `u32` field out of a `serde_json::Value` object.
fn pull_u32(raw: &serde_json::Value, key: &str) -> ReducerResult<u32> {
    raw.get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            ReducerError::Invariant(format!(
                "maverick_v2 raw payload: missing or non-number `{key}`"
            ))
        })
        .and_then(|n| {
            u32::try_from(n).map_err(|_| {
                ReducerError::Invariant(format!("maverick_v2 raw payload: `{key}` overflows u32"))
            })
        })
}

/// Quote a single hop on a Maverick V2 pool given its `Maverick` `PoolState`
/// snapshot. Returns the hop's output amount; caller is responsible for fee
/// accounting and balance changes.
///
/// **Simplification** — single active-bin constant-product; see module docs
/// for the deferred mode-aware bin-walking work and the expected `raw`
/// payload shape.
///
/// Errors with `Invariant` on:
///   * `PoolState` variant other than `Maverick`,
///   * `raw` payload missing `reserve_in` / `reserve_out` / `fee_bp` fields,
///   * `fee_bp >= 10_000`,
///   * `reserve_in == 0` or `reserve_out == 0`,
///   * any `U256` overflow.
pub(super) fn quote_swap_hop(
    _state: &WalletState,
    _ctx: &EvalContext,
    _swap: &SwapAction,
    pool_state: &PoolState,
    amount_in: U256,
) -> ReducerResult<U256> {
    let PoolState::Maverick { mode: _, raw } = pool_state else {
        return Err(ReducerError::Invariant(
            "non-Maverick pool_state for maverick_v2 swap".into(),
        ));
    };
    if amount_in.is_zero() {
        return Ok(U256::ZERO);
    }

    let reserve_in = pull_u256(raw, "reserve_in")?;
    let reserve_out = pull_u256(raw, "reserve_out")?;
    let fee_bp = pull_u32(raw, "fee_bp")?;

    if fee_bp >= 10_000 {
        return Err(ReducerError::Invariant(format!(
            "maverick_v2 fee_bp {fee_bp} out of range (must be < 10000)"
        )));
    }
    if reserve_in.is_zero() || reserve_out.is_zero() {
        return Err(ReducerError::Invariant(
            "maverick_v2: zero active-bin reserve".into(),
        ));
    }

    let fee_multiplier = U256::from(10_000u32 - fee_bp);
    let amount_in_after_fee = amount_in
        .checked_mul(fee_multiplier)
        .ok_or_else(|| ReducerError::Invariant("maverick_v2: in-fee overflow".into()))?
        / U256::from(10_000u32);

    // out = reserve_out * in_after_fee / (reserve_in + in_after_fee)
    let numerator = reserve_out
        .checked_mul(amount_in_after_fee)
        .ok_or_else(|| ReducerError::Invariant("maverick_v2: numerator overflow".into()))?;
    let denominator = reserve_in
        .checked_add(amount_in_after_fee)
        .ok_or_else(|| ReducerError::Invariant("maverick_v2: denominator overflow".into()))?;
    if denominator.is_zero() {
        return Err(ReducerError::Invariant(
            "maverick_v2: zero denominator".into(),
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
    use serde_json::json;

    /// `1_000 / 1_000` reserves, zero fee, `100` in →
    /// `1_000 * 100 / 1_100 = 90`.
    #[test]
    fn active_bin_quote_matches_constant_product() {
        let pool = PoolState::Maverick {
            mode: "mode_static".into(),
            raw: json!({
                "reserve_in":  "1000",
                "reserve_out": "1000",
                "fee_bp":      0
            }),
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

    /// Fee applied on `amount_in`. `30 bp` on `100` in → `99` after fee;
    /// `1_000 * 99 / 1_099 = 90` (integer floor).
    #[test]
    fn fee_subtracts_from_amount_in() {
        let pool = PoolState::Maverick {
            mode: "mode_right".into(),
            raw: json!({
                "reserve_in":  "1000",
                "reserve_out": "1000",
                "fee_bp":      30
            }),
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

    /// Missing field in raw payload surfaces as `Invariant` with the field
    /// name embedded for diagnosability.
    #[test]
    fn missing_field_errors_with_field_name() {
        let pool = PoolState::Maverick {
            mode: "mode_static".into(),
            raw: json!({
                "reserve_in":  "1000",
                "fee_bp":      0
                // reserve_out missing
            }),
        };
        let err = quote_swap_hop(
            &empty_state(),
            &ctx(),
            &dummy_swap(),
            &pool,
            U256::from(1u64),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("reserve_out")));
    }

    /// Zero `amount_in` short-circuits to zero out.
    #[test]
    fn zero_amount_in_returns_zero() {
        let pool = PoolState::Maverick {
            mode: "mode_static".into(),
            raw: json!({}),
        };
        let out = quote_swap_hop(&empty_state(), &ctx(), &dummy_swap(), &pool, U256::ZERO).unwrap();
        assert_eq!(out, U256::ZERO);
    }

    /// Non-`Maverick` variants rejected.
    #[test]
    fn rejects_non_maverick_pool_state() {
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
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("Maverick")));
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
            venue: AmmVenue::MaverickV2 {
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
