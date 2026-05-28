//! Cross-venue perpetuals math primitives.
//!
//! Every venue's `unrealized_pnl` / `funding_accrued` is algebraically
//! identical — the only venue-specific knob is `liquidation_price` (which
//! differs in maintenance-margin / cross-vs-isolated handling) and a venue
//! tag carried in error messages. We hoist the common computations here so
//! the per-venue files (`hyperliquid.rs`, `gmx_v2.rs`, ...) stay thin and
//! provably consistent.
//!
//! ## Arithmetic backend
//!
//! `simulation_state::primitives::Decimal` is a `String` newtype safe for
//! transport / serde but lacking arithmetic. We bridge to
//! `rust_decimal::Decimal` for math (parse → arithmetic → format), matching
//! the convention already used by `helpers::derived` for HF / LTV / `PnL`.
//!
//! ## Liquidation-price models
//!
//! Two flavors are exposed:
//!   - [`liquidation_price_simple`] — single-asset isolated-margin closed
//!     form: `entry ± (free_margin − maintenance_margin) / size_base`.
//!     Used by Hyperliquid / Aevo / Vertex / `DyDx` V4 / Drift.
//!   - [`liquidation_price_deferred`] — returns
//!     `UnsupportedProtocol { … "deferred — full liq price requires …" }`
//!     for venues whose accurate liquidation price requires multi-collateral
//!     / funding / borrowing-fee accumulator state we cannot soundly derive
//!     from `OpenPerpLiveInputs` today. GMX V2 / Synthetix V3 / Jupiter
//!     Perps fall in this bucket; the field is computed off-chain by the
//!     venue's official subgraph / SDK and surfaced via a separate `LiveField`
//!     on the existing position in subsequent batches.

#![allow(dead_code)]

use std::str::FromStr;

use rust_decimal::Decimal as RustDecimal;
use simulation_state::primitives::{Decimal, Price, SignedI256, U256};

use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs, PerpVenue, SizeSpec};
use crate::error::{ReducerError, ReducerResult};

// ---------------------------------------------------------------------------
// Decimal conversion helpers (mirrors `helpers::derived` patterns).
// ---------------------------------------------------------------------------

/// Parse a `simulation_state::Decimal` (`String` newtype) into a
/// `rust_decimal::Decimal` for arithmetic.
pub(super) fn parse_decimal(d: &Decimal) -> ReducerResult<RustDecimal> {
    RustDecimal::from_str(d.as_str()).map_err(|e| {
        ReducerError::Invariant(format!("invalid Decimal string {:?}: {e}", d.as_str()))
    })
}

/// Convert a `U256` to `rust_decimal::Decimal`. Mirrors `helpers::derived`
/// — fails with `Invariant` if the value exceeds `rust_decimal` range
/// (~7.9e28). For typical perp `size_base` (≤ 1e30 raw with 18 decimals)
/// this covers all realistic positions.
pub(super) fn u256_to_decimal(amount: U256) -> ReducerResult<RustDecimal> {
    let s = amount.to_string();
    RustDecimal::from_str(&s)
        .map_err(|e| ReducerError::Invariant(format!("U256 {s} exceeds rust_decimal range: {e}")))
}

/// Render a `rust_decimal::Decimal` back into the state-side `Decimal`
/// (`String` newtype) using its canonical (no-trailing-zero) form.
pub(super) fn decimal_to_state(d: RustDecimal) -> Decimal {
    Decimal::new(d.normalize().to_string())
}

/// Truncate a `rust_decimal::Decimal` toward zero and render it as a
/// `SignedI256`. Used by `unrealized_pnl` / `funding_accrued` — venues book
/// at integer denom so sub-integer fractions are dropped (matches Aave /
/// Hyperliquid realized-PnL accounting).
fn decimal_trunc_to_signed(d: RustDecimal) -> ReducerResult<SignedI256> {
    let s = d.trunc().to_string();
    SignedI256::from_dec_str(&s)
        .map_err(|e| ReducerError::Invariant(format!("decimal {s} overflows SignedI256: {e}")))
}

/// Truncate a `rust_decimal::Decimal` toward zero and render it as `U256`.
/// Negative input → `Invariant` (margin amounts are non-negative).
fn decimal_trunc_to_u256(d: RustDecimal, label: &str) -> ReducerResult<U256> {
    if d.is_sign_negative() {
        return Err(ReducerError::Invariant(format!(
            "{label}: expected non-negative, got {d}"
        )));
    }
    let s = d.trunc().to_string();
    U256::from_str_radix(&s, 10)
        .map_err(|e| ReducerError::Invariant(format!("{label}: U256 parse of {s} failed: {e}")))
}

// ---------------------------------------------------------------------------
// Size resolution
// ---------------------------------------------------------------------------

/// Resolve a `SizeSpec` to a base-asset `U256` size given the live mark
/// price and (for `LeverageImplied`) the user-specified leverage.
///
/// * `BaseAmount { amount }` — passthrough.
/// * `QuoteAmount { amount_usd }` — `amount_usd / mark_price`.
/// * `LeverageImplied { collateral, leverage }` — `collateral × leverage /
///   mark_price`.
///
/// All math truncates toward zero (venues round size to the nearest tick
/// off-chain; we leave any tick alignment to the venue itself).
pub(super) fn resolve_size_base(spec: &SizeSpec, mark: &Price) -> ReducerResult<U256> {
    match spec {
        SizeSpec::BaseAmount { amount } => Ok(*amount),
        SizeSpec::QuoteAmount { amount_usd } => {
            let mark_d = parse_decimal(mark)?;
            if mark_d.is_zero() {
                return Err(ReducerError::Invariant(
                    "resolve_size_base: mark price is zero".into(),
                ));
            }
            let usd = u256_to_decimal(*amount_usd)?;
            decimal_trunc_to_u256(usd / mark_d, "size_base from QuoteAmount")
        }
        SizeSpec::LeverageImplied {
            collateral,
            leverage,
        } => {
            let mark_d = parse_decimal(mark)?;
            if mark_d.is_zero() {
                return Err(ReducerError::Invariant(
                    "resolve_size_base: mark price is zero".into(),
                ));
            }
            let coll = u256_to_decimal(*collateral)?;
            let lev = parse_decimal(leverage)?;
            decimal_trunc_to_u256(coll * lev / mark_d, "size_base from LeverageImplied")
        }
    }
}

/// Compute the notional value of a position (`size_base × mark_price`) in
/// quote units (USD-denominated for every supported venue).
pub(super) fn notional_usd(size_base: U256, mark: &Price) -> ReducerResult<RustDecimal> {
    let mark_d = parse_decimal(mark)?;
    let size = u256_to_decimal(size_base)?;
    Ok(size * mark_d)
}

// ---------------------------------------------------------------------------
// Common formulas
// ---------------------------------------------------------------------------

/// Common `required_initial_margin` formula — used by every venue.
///
/// ```text
///   notional       = size_base × mark_price
///   required_margin = notional / leverage + taker_fee_bp × notional / 10_000
/// ```
///
/// `leverage` comes from `action.leverage` (Hyperliquid / GMX V2 / `DyDx` V4
/// all allow user-specified leverage up to the venue's `max_leverage`).
/// `fee_taker_bp` is venue-quoted via the `LiveField`. Returns the margin in
/// the venue's quote denom (USDC for most, USDC.e for Vertex, `USDe` for Aevo).
pub(super) fn required_initial_margin_common(
    venue_label: &str,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<U256> {
    let mark = parse_decimal(&live.mark_price.value)?;
    if mark.is_zero() {
        return Err(ReducerError::Invariant(format!(
            "{venue_label}: mark price is zero"
        )));
    }
    let leverage = parse_decimal(&action.leverage)?;
    if leverage.is_zero() {
        return Err(ReducerError::Invariant(format!(
            "{venue_label}: leverage is zero"
        )));
    }
    let max_lev = parse_decimal(&live.max_leverage.value)?;
    if !max_lev.is_zero() && leverage > max_lev {
        return Err(ReducerError::Invariant(format!(
            "{venue_label}: leverage {leverage} exceeds venue max {max_lev}"
        )));
    }
    let size_base = resolve_size_base(&action.size, &live.mark_price.value)?;
    let size = u256_to_decimal(size_base)?;
    let notional = size * mark;
    let fee_taker = RustDecimal::from(live.fee_taker_bp.value);
    let bp_denom = RustDecimal::from(10_000_u32);
    let margin = notional / leverage + fee_taker * notional / bp_denom;
    decimal_trunc_to_u256(margin, &format!("{venue_label}::required_initial_margin"))
}

/// Common single-asset isolated-margin `liquidation_price` formula.
///
/// ```text
///   maintenance_margin = notional × maintenance_bp / 10_000
///   liq_price          = entry ± (free_margin − maintenance_margin) / size_base
/// ```
///
/// `free_margin` comes from `live.user_account_state.free_margin_usd`.
/// `entry` ≈ `mark_price` at open (the simulated swap is the spot price).
/// Returns `Ok(None)` if `size_base = 0`. Returns `Decimal::zero()` (clipped)
/// if the formula would push the price negative — matches the convention used
/// by `helpers::derived::recompute_liq_price`.
pub(super) fn liquidation_price_simple(
    venue_label: &str,
    action: &OpenPerpAction,
    live: &OpenPerpLiveInputs,
) -> ReducerResult<Option<Price>> {
    let size_base = resolve_size_base(&action.size, &live.mark_price.value)?;
    if size_base == U256::ZERO {
        return Ok(None);
    }
    let mark = parse_decimal(&live.mark_price.value)?;
    let size = u256_to_decimal(size_base)?;
    let notional = size * mark;
    let maintenance =
        notional * RustDecimal::from(live.maintenance_bp.value) / RustDecimal::from(10_000_u32);
    let free_margin = u256_to_decimal(live.user_account_state.value.free_margin_usd)?;
    if size.is_zero() {
        return Err(ReducerError::Invariant(format!(
            "{venue_label}: size_base resolved to zero (post-quote-check)"
        )));
    }
    let buffer = (free_margin - maintenance) / size;
    let liq = match &action.side {
        simulation_state::position::PerpSide::Long => mark - buffer,
        simulation_state::position::PerpSide::Short => mark + buffer,
    };
    let liq = if liq.is_sign_negative() {
        RustDecimal::ZERO
    } else {
        liq
    };
    Ok(Some(decimal_to_state(liq)))
}

/// Sentinel for venues whose accurate liquidation price requires
/// venue-specific accumulator state (funding / borrowing fees / multi-
/// collateral oracle blend) we cannot soundly compute from
/// `OpenPerpLiveInputs` alone. Returns
/// `UnsupportedProtocol { … "deferred — see venue docs" }`.
///
/// Venues currently using this stub: GMX V2 (`PositionUtils.getLiquidationPrice`
/// needs funding+borrowing fee state), Synthetix V3 (multi-collateral debt
/// pool), Jupiter Perps (JLP pool dynamics).
pub(super) fn liquidation_price_deferred(
    venue_label: &str,
    venue: &PerpVenue,
) -> ReducerResult<Option<Price>> {
    let _ = venue;
    Err(ReducerError::UnsupportedProtocol {
        action: "open_perp.liquidation_price".into(),
        protocol: format!("{venue_label} — deferred (requires venue subgraph state)"),
    })
}

/// Common `unrealized_pnl` formula:
///
/// ```text
///   pnl = size_base × (mark − entry) × side_sign
/// ```
///
/// `side_sign = +1` (long) or `−1` (short). Truncates toward zero (venues
/// book at integer denom, matching `helpers::derived::recompute_perp_pnl`).
pub(super) fn unrealized_pnl_common(
    size_base: U256,
    entry: &Price,
    mark: &Price,
    is_long: bool,
) -> ReducerResult<SignedI256> {
    let entry_d = parse_decimal(entry)?;
    let mark_d = parse_decimal(mark)?;
    let size = u256_to_decimal(size_base)?;
    let diff = if is_long {
        mark_d - entry_d
    } else {
        entry_d - mark_d
    };
    decimal_trunc_to_signed(size * diff)
}

/// Common `funding_accrued` formula:
///
/// ```text
///   funding = size_base × funding_rate × hours_elapsed / 24
/// ```
///
/// `funding_rate` is the venue's natural daily rate (Hyperliquid hourly
/// rates are pre-summed by the orchestrator into a daily-equivalent before
/// the `LiveField` is published). Truncates toward zero. Positive result =
/// funding accrued *to* the position; negative = paid *from*.
pub(super) fn funding_accrued_common(
    size_base: U256,
    funding_rate: &Decimal,
    hours_elapsed: u32,
) -> ReducerResult<SignedI256> {
    let rate = parse_decimal(funding_rate)?;
    let size = u256_to_decimal(size_base)?;
    let hours = RustDecimal::from(hours_elapsed);
    let twenty_four = RustDecimal::from(24_u32);
    decimal_trunc_to_signed(size * rate * hours / twenty_four)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
    use simulation_state::position::{MarginMode, PerpSide};
    use simulation_state::primitives::{Address, ChainId, MarketRef, Time, VenueRef};
    use simulation_state::token::{TokenKey, TokenRef};
    use std::str::FromStr;

    use crate::action::perp::{OpenPerpAction, OpenPerpLiveInputs, PerpAccountState, PerpVenue};

    fn live<T>(value: T) -> LiveField<T> {
        LiveField::new(
            value,
            DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "ETH-PERP-MARK".into(),
            },
            Time::from_unix(1_738_000_000),
        )
    }

    fn usdc_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        })
    }

    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    fn mk_open(
        side: PerpSide,
        leverage_str: &str,
        size: SizeSpec,
        mark_str: &str,
        free_margin_usd: u64,
        taker_bp: u32,
        maintenance_bp: u32,
        max_leverage_str: &str,
    ) -> (OpenPerpAction, OpenPerpLiveInputs) {
        let mark = Decimal::new(mark_str);
        let live_inputs = OpenPerpLiveInputs {
            mark_price: live(mark.clone()),
            oracle_price: live(mark),
            funding_rate: live(Decimal::new("0")),
            available_oi: live(U256::from(u128::MAX)),
            max_leverage: live(Decimal::new(max_leverage_str)),
            initial_margin_bp: live(0),
            maintenance_bp: live(maintenance_bp),
            fee_taker_bp: live(taker_bp),
            fee_maker_bp: live(0),
            user_account_state: live(PerpAccountState {
                total_collateral_usd: U256::from(free_margin_usd),
                used_margin_usd: U256::ZERO,
                free_margin_usd: U256::from(free_margin_usd),
                open_positions: vec![],
            }),
        };
        let action = OpenPerpAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: MarketRef {
                symbol: "ETH-PERP".into(),
                venue: VenueRef::new("hyperliquid"),
            },
            side,
            size,
            leverage: Decimal::new(leverage_str),
            collateral: (usdc_ref(), U256::from(free_margin_usd)),
            margin_mode: MarginMode::Isolated,
            slippage_bp: 50,
            reduce_only: false,
            live_inputs: live_inputs.clone(),
        };
        (action, live_inputs)
    }

    /// 1 ETH × $3000 = $3000 notional / leverage 5 = $600.
    /// Plus 5 bp taker fee: 5 × 3000 / `10_000` = 1.5 → trunc → $601.
    #[test]
    fn required_initial_margin_basic() {
        let (action, live_inputs) = mk_open(
            PerpSide::Long,
            "5",
            SizeSpec::BaseAmount {
                amount: U256::from(1_u64),
            },
            "3000",
            10_000,
            5,
            200,
            "50",
        );
        let m = required_initial_margin_common("hyperliquid", &action, &live_inputs).unwrap();
        // 600 + trunc(1.5) = 601
        assert_eq!(m, U256::from(601_u64));
    }

    /// Leverage > `max_leverage` → Invariant.
    #[test]
    fn required_initial_margin_leverage_cap_rejected() {
        let (action, live_inputs) = mk_open(
            PerpSide::Long,
            "100", // requested
            SizeSpec::BaseAmount {
                amount: U256::from(1_u64),
            },
            "3000",
            10_000,
            5,
            200,
            "50", // max
        );
        let err = required_initial_margin_common("hyperliquid", &action, &live_inputs).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// Long: size 2 × mark 3000 = 6000 notional. Maintenance bp 200 → 120.
    /// `free_margin` 10000 → buffer = (10000 − 120) / 2 = 4940. liq = 3000 −
    /// 4940 = −1940 → clip → 0.
    #[test]
    fn liquidation_price_long_clips_to_zero_when_overcollateralized() {
        let (action, live_inputs) = mk_open(
            PerpSide::Long,
            "5",
            SizeSpec::BaseAmount {
                amount: U256::from(2_u64),
            },
            "3000",
            10_000,
            5,
            200,
            "50",
        );
        let liq = liquidation_price_simple("hyperliquid", &action, &live_inputs).unwrap();
        assert_eq!(liq.unwrap().as_str(), "0");
    }

    /// Long with thin margin: size 1 @ mark 3000, maintenance bp 200 → 60.
    /// `free_margin` = 100 → buffer = (100 − 60) / 1 = 40. liq = 3000 − 40 =
    /// 2960.
    #[test]
    fn liquidation_price_long_thin_margin() {
        let (action, live_inputs) = mk_open(
            PerpSide::Long,
            "30",
            SizeSpec::BaseAmount {
                amount: U256::from(1_u64),
            },
            "3000",
            100,
            5,
            200,
            "50",
        );
        let liq = liquidation_price_simple("hyperliquid", &action, &live_inputs).unwrap();
        assert_eq!(liq.unwrap().as_str(), "2960");
    }

    /// Short: liq = mark + buffer. size 1 @ mark 3000, maintenance bp 200
    /// → 60. `free_margin` = 100 → buffer = 40 → liq = 3040.
    #[test]
    fn liquidation_price_short_thin_margin() {
        let (action, live_inputs) = mk_open(
            PerpSide::Short,
            "30",
            SizeSpec::BaseAmount {
                amount: U256::from(1_u64),
            },
            "3000",
            100,
            5,
            200,
            "50",
        );
        let liq = liquidation_price_simple("hyperliquid", &action, &live_inputs).unwrap();
        assert_eq!(liq.unwrap().as_str(), "3040");
    }

    /// `QuoteAmount` size resolution: $6000 at mark $3000 → 2 ETH.
    #[test]
    fn resolve_size_base_quote_amount() {
        let s = resolve_size_base(
            &SizeSpec::QuoteAmount {
                amount_usd: U256::from(6_000_u64),
            },
            &Decimal::new("3000"),
        )
        .unwrap();
        assert_eq!(s, U256::from(2_u64));
    }

    /// `LeverageImplied`: $1000 × 6x / $3000 = 2 ETH.
    #[test]
    fn resolve_size_base_leverage_implied() {
        let s = resolve_size_base(
            &SizeSpec::LeverageImplied {
                collateral: U256::from(1_000_u64),
                leverage: Decimal::new("6"),
            },
            &Decimal::new("3000"),
        )
        .unwrap();
        assert_eq!(s, U256::from(2_u64));
    }

    /// `liquidation_price_deferred` returns the standardised `UnsupportedProtocol`.
    #[test]
    fn liquidation_price_deferred_signals_unsupported() {
        let venue = PerpVenue::GmxV2 {
            chain: ChainId::ethereum_mainnet(),
        };
        let err = liquidation_price_deferred("gmx_v2", &venue).unwrap_err();
        match err {
            ReducerError::UnsupportedProtocol { action, protocol } => {
                assert_eq!(action, "open_perp.liquidation_price");
                assert!(protocol.contains("deferred"));
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
    }
}
