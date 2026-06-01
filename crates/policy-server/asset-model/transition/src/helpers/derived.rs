//! Recompute `LiveField`s whose `source: DataSource::DerivedFrom { .. }` —
//! values computed from other on-chain primitives rather than fetched.
//!
//! Sync orchestrator calls these for `DerivedFrom` `LiveField`s it discovers
//! during state walks. Reducers call them inline when a write to a primitive
//! invalidates a derived value (e.g. supplying USDC to Aave changes the
//! account's `health_factor`).
//!
//! ## Decimal arithmetic
//!
//! `simulation_state::primitives::Decimal` is a `String` newtype — safe for
//! transport / serde but lacking arithmetic. We bridge to
//! [`rust_decimal::Decimal`] for the math (parse → arithmetic → format) so
//! financial precision is preserved (no `f64` rounding artifacts).
//!
//! ## Price injection
//!
//! `LendingAccount::collaterals` / `debts` carry only `(TokenRef, U256)` —
//! USD prices and liquidation thresholds live in the surrounding
//! `BorrowLiveInputs` / `SupplyLiveInputs` (`asset_price_usd`, `reserve_state`).
//! The HF / LTV recompute functions therefore take those values explicitly as
//! parallel slices keyed by `TokenRef`; callers (`effect/lending/*.rs`) are
//! responsible for assembling them from `live_inputs`.

use std::str::FromStr;

use rust_decimal::Decimal as RustDecimal;
use simulation_state::position::{LendingAccount, PerpPosition, PerpSide};
use simulation_state::primitives::{Decimal, Price, SignedI256, Time, U256};
use simulation_state::token::TokenRef;

use crate::error::{ReducerError, ReducerResult};

// ---------------------------------------------------------------------------
// Decimal conversion helpers
// ---------------------------------------------------------------------------

/// Parse a `simulation_state::Decimal` (String newtype) into a
/// `rust_decimal::Decimal` for arithmetic. Returns `Invariant` on parse
/// failure — the string should have been validated at construction time.
fn parse_decimal(d: &Decimal) -> ReducerResult<RustDecimal> {
    RustDecimal::from_str(d.as_str()).map_err(|e| {
        ReducerError::Invariant(format!("invalid Decimal string {:?}: {e}", d.as_str()))
    })
}

/// Convert a `U256` amount into `rust_decimal::Decimal`. The value must fit
/// into `i128` (`rust_decimal`'s underlying mantissa). For typical token
/// balances (decimals ≤ 18) this covers ~10^20 USDC, which is far above any
/// realistic wallet position.
fn u256_to_decimal(amount: U256) -> ReducerResult<RustDecimal> {
    // `rust_decimal` max ≈ 7.9e28; U256 max ≈ 1.16e77. Convert via the
    // intermediate string representation to avoid silent truncation when the
    // value exceeds i128.
    let s = amount.to_string();
    RustDecimal::from_str(&s)
        .map_err(|e| ReducerError::Invariant(format!("U256 {s} exceeds rust_decimal range: {e}")))
}

/// Render a `rust_decimal::Decimal` back into the `simulation_state::Decimal`
/// String newtype using its canonical (no-trailing-zero) form.
fn decimal_to_state(d: RustDecimal) -> Decimal {
    Decimal::new(d.normalize().to_string())
}

/// Look up a price for `token` in a `&[(TokenRef, Decimal)]` slice. Returns
/// `Invariant` if the price is missing — callers must pre-populate every
/// referenced token.
fn lookup_price(token: &TokenRef, table: &[(TokenRef, Decimal)]) -> ReducerResult<RustDecimal> {
    table
        .iter()
        .find(|(t, _)| t == token)
        .ok_or_else(|| ReducerError::Invariant(format!("missing price for token {:?}", token.key)))
        .and_then(|(_, p)| parse_decimal(p))
}

/// Look up a liquidation-threshold (in basis points) for `token`. Returns
/// `Invariant` on miss — same contract as `lookup_price`.
fn lookup_lt_bp(token: &TokenRef, table: &[(TokenRef, u32)]) -> ReducerResult<u32> {
    table
        .iter()
        .find(|(t, _)| t == token)
        .map(|(_, bp)| *bp)
        .ok_or_else(|| {
            ReducerError::Invariant(format!(
                "missing liquidation threshold for token {:?}",
                token.key
            ))
        })
}

// ---------------------------------------------------------------------------
// HF / LTV
// ---------------------------------------------------------------------------

/// Sentinel value returned for `recompute_health_factor` when total debt is
/// zero. Mirrors Aave V3 behaviour (`type(uint256).max`). We use `999999999`
/// because the field is a `Decimal` String and downstream policy needs a
/// comparable numeric — `"inf"` would not round-trip through arithmetic.
const HF_INFINITY: &str = "999999999";

/// Recompute `LendingAccount::health_factor` from its `collaterals` and
/// `debts` plus current oracle prices.
///
/// PDF §5 formula:
/// ```text
///   HF = Σ(collateral_i.amount × collateral_i.price_usd × LT_i_bp / 10_000)
///        ────────────────────────────────────────────────────────────────
///                  Σ(debt_j.amount × debt_j.price_usd)
/// ```
///
/// `account.collaterals` / `debts` only carry `(TokenRef, U256)` — USD prices
/// and per-asset liquidation thresholds (in basis points) must be injected
/// from the surrounding `BorrowLiveInputs::reserve_state` /
/// `asset_price_usd` `LiveField`s. The caller assembles the parallel slices.
///
/// Returns the sentinel `HF_INFINITY` when total debt is zero (Aave's
/// `type(uint256).max` analogue).
///
/// # Errors
///
/// Returns `Invariant` if a referenced price or LT is missing from the
/// inject tables, or if a price string fails to parse.
pub fn recompute_health_factor(
    account: &LendingAccount,
    collateral_prices: &[(TokenRef, Decimal)],
    debt_prices: &[(TokenRef, Decimal)],
    liquidation_thresholds: &[(TokenRef, u32)],
    _now: Time,
) -> ReducerResult<Decimal> {
    let bp_denom = RustDecimal::from(10_000_u32);
    let mut weighted_collat = RustDecimal::ZERO;
    for (token, amount) in &account.collaterals {
        let price = lookup_price(token, collateral_prices)?;
        let lt_bp = lookup_lt_bp(token, liquidation_thresholds)?;
        let amt = u256_to_decimal(*amount)?;
        let lt_factor = RustDecimal::from(lt_bp) / bp_denom;
        weighted_collat += amt * price * lt_factor;
    }

    let mut total_debt = RustDecimal::ZERO;
    for (token, amount, _rate_mode) in &account.debts {
        let price = lookup_price(token, debt_prices)?;
        let amt = u256_to_decimal(*amount)?;
        total_debt += amt * price;
    }

    if total_debt.is_zero() {
        return Ok(Decimal::new(HF_INFINITY));
    }

    Ok(decimal_to_state(weighted_collat / total_debt))
}

/// Recompute `LendingAccount::ltv` from current collateral / debt USD.
///
/// PDF §5 formula:
/// ```text
///   LTV = Σ(debt_j.amount × debt_j.price_usd)
///         ──────────────────────────────────────
///         Σ(collateral_i.amount × collat_i.price_usd)
/// ```
///
/// Returns `Decimal::zero()` when total debt is zero (no risk; LTV trivially
/// 0). Returns `Invariant` when total *collateral* is zero but debt is not
/// (a degenerate state that the reducer should never produce).
///
/// # Errors
///
/// Returns `Invariant` if a referenced price is missing, fails to parse, or
/// if the account has debt against zero collateral.
pub fn recompute_ltv(
    account: &LendingAccount,
    collateral_prices: &[(TokenRef, Decimal)],
    debt_prices: &[(TokenRef, Decimal)],
    _now: Time,
) -> ReducerResult<Decimal> {
    let mut total_collat = RustDecimal::ZERO;
    for (token, amount) in &account.collaterals {
        let price = lookup_price(token, collateral_prices)?;
        let amt = u256_to_decimal(*amount)?;
        total_collat += amt * price;
    }

    let mut total_debt = RustDecimal::ZERO;
    for (token, amount, _rate_mode) in &account.debts {
        let price = lookup_price(token, debt_prices)?;
        let amt = u256_to_decimal(*amount)?;
        total_debt += amt * price;
    }

    if total_debt.is_zero() {
        return Ok(Decimal::zero());
    }
    if total_collat.is_zero() {
        return Err(ReducerError::Invariant(
            "LTV undefined: debt > 0 with zero collateral".into(),
        ));
    }
    Ok(decimal_to_state(total_debt / total_collat))
}

// ---------------------------------------------------------------------------
// Perp — PnL / liquidation price
// ---------------------------------------------------------------------------

/// Recompute `PerpPosition::unrealized_pnl` from entry / mark / size.
///
/// PDF §5 formula (long):
/// ```text
///   pnl = size_base × (mark_price - entry_price)
/// ```
/// For shorts the sign flips: `pnl = size_base × (entry_price - mark_price)`.
/// The result is quoted in the position's `notional_usd` denom (i.e. price ×
/// size unit cancellation). `size_base` carries the base-asset decimals; the
/// reducer wires it raw without re-scaling, matching how `notional_usd` is
/// emitted by the venue.
///
/// Returns `SignedI256` — `rust_decimal`'s signed result is rendered as an
/// integer (mantissa-only, post-truncation) since `SignedI256` cannot carry
/// fractional units. For the typical USDC-quoted perp this means `PnL` is
/// rounded toward zero at the 1-wei boundary, identical to how venues book
/// realized `PnL`.
///
/// # Errors
///
/// Returns `Invariant` if `entry_price` / `mark_price` fail to parse, if the
/// `size_base` exceeds `rust_decimal` range (~7.9e28), or if the final
/// result overflows `i128` (the intermediate type before `SignedI256`).
pub fn recompute_perp_pnl(
    position: &PerpPosition,
    mark_price: &Price,
    _now: Time,
) -> ReducerResult<SignedI256> {
    let mark = parse_decimal(mark_price)?;
    let entry = parse_decimal(&position.entry_price)?;
    let size = u256_to_decimal(position.size_base)?;

    let diff = match position.side {
        PerpSide::Long => mark - entry,
        PerpSide::Short => entry - mark,
    };
    let pnl = size * diff;
    // Truncate toward zero — venues book realized PnL at integer denom.
    let pnl_int = pnl.trunc();

    // Render via decimal-string → SignedI256. `SignedI256::from_dec_str`
    // accepts an optional leading sign, matching `rust_decimal`'s string
    // form for negatives.
    let s = pnl_int.to_string();
    SignedI256::from_dec_str(&s)
        .map_err(|e| ReducerError::Invariant(format!("PnL {s} overflows SignedI256: {e}")))
}

/// Recompute `PerpPosition::liq_price` from collateral, size, and
/// (implicit) venue margin params.
///
/// Simple fallback formula (PDF §5):
/// ```text
///   liq_price = entry_price ∓ (free_margin / size_base)
/// ```
/// Long uses `-`, short uses `+`. `free_margin` is approximated as the sum
/// of collateral amounts treated 1-USD-each — venue-accurate calculation
/// (mark-based margin balance + maintenance ratio) lives in
/// `effect/perp/<venue>.rs` and overrides this fallback.
///
/// Returns `Ok(None)` when `size_base == 0` (no position → no liquidation),
/// matching how venues mark a fully closed slot. Returns
/// `Ok(Some(Decimal::zero()))` when the computed price would go negative
/// (long position with overwhelming collateral) — clipped at zero since
/// `liq_price` cannot be negative.
///
/// # Errors
///
/// Returns `Invariant` if `entry_price` fails to parse, if collateral /
/// `size_base` exceed `rust_decimal` range, or if any internal arithmetic
/// produces a non-finite intermediate.
pub fn recompute_liq_price(position: &PerpPosition, _now: Time) -> ReducerResult<Option<Price>> {
    if position.size_base == U256::ZERO {
        return Ok(None);
    }

    let entry = parse_decimal(&position.entry_price)?;
    let size = u256_to_decimal(position.size_base)?;

    // free_margin ≈ Σ collateral amount, treated as USD-denominated. Venue
    // adapters override with accurate margin balance.
    let mut free_margin = RustDecimal::ZERO;
    for (_token, amount) in &position.collateral {
        free_margin += u256_to_decimal(*amount)?;
    }

    let buffer = free_margin / size;
    let liq = match position.side {
        PerpSide::Long => entry - buffer,
        PerpSide::Short => entry + buffer,
    };
    let liq = if liq < RustDecimal::ZERO {
        RustDecimal::ZERO
    } else {
        liq
    };

    Ok(Some(decimal_to_state(liq)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::ToPrimitive;
    use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
    use simulation_state::position::lending::LendingAccount;
    use simulation_state::position::perp::{MarginMode, PerpPosition, PerpSide};
    use simulation_state::primitives::{Address, ChainId, MarketRef, VenueRef};
    use simulation_state::token::{RateMode, TokenKey, TokenRef};
    use std::str::FromStr;

    fn usdc_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        })
    }

    fn weth_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
        })
    }

    fn dummy_live<T>(value: T) -> LiveField<T> {
        LiveField::new(
            value,
            DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "TEST/USD".into(),
            },
            Time::from_unix(1_738_000_000),
        )
    }

    fn mock_account(
        collaterals: Vec<(TokenRef, U256)>,
        debts: Vec<(TokenRef, U256, RateMode)>,
    ) -> LendingAccount {
        LendingAccount {
            market: MarketRef {
                symbol: "USDC".into(),
                venue: VenueRef::new("aave_v3"),
            },
            collaterals,
            debts,
            emode: None,
            is_isolated: false,
            health_factor: dummy_live(Decimal::new("0")),
            ltv: dummy_live(Decimal::new("0")),
            liquidation_threshold: dummy_live(Decimal::new("0")),
        }
    }

    // -- HF -------------------------------------------------------------

    /// PDF §11 fixture #6: Aave V3 borrow Optimism — `user_state_before`
    /// reports HF = 2.4. We reverse-engineer one valid (collat × LT, debt)
    /// pair that yields 2.4 exactly: 2400 USDC collateral @ $1, LT 80%,
    /// against 800 USDC debt @ $1 → (2400 × 1 × 0.8) / (800 × 1) = 2.4.
    #[test]
    fn hf_aave_v3_borrow_fixture_24() {
        let usdc = usdc_ref();
        let account = mock_account(
            vec![(usdc.clone(), U256::from(2_400_000_000_u64))],
            vec![(
                usdc.clone(),
                U256::from(800_000_000_u64),
                RateMode::Variable,
            )],
        );
        let collat_prices = vec![(usdc.clone(), Decimal::new("1"))];
        let debt_prices = vec![(usdc.clone(), Decimal::new("1"))];
        let lt = vec![(usdc, 8_000_u32)]; // 80% LT

        let hf = recompute_health_factor(
            &account,
            &collat_prices,
            &debt_prices,
            &lt,
            Time::from_unix(0),
        )
        .unwrap();
        assert_eq!(hf.as_str(), "2.4");
    }

    /// Zero debt → infinite HF sentinel.
    #[test]
    fn hf_zero_debt_is_infinity_sentinel() {
        let usdc = usdc_ref();
        let account = mock_account(vec![(usdc.clone(), U256::from(1_000_000_u64))], vec![]);
        let prices = vec![(usdc.clone(), Decimal::new("1"))];
        let lt = vec![(usdc, 8_000)];

        let hf = recompute_health_factor(&account, &prices, &[], &lt, Time::from_unix(0)).unwrap();
        assert_eq!(hf.as_str(), HF_INFINITY);
    }

    /// Multi-asset weighted HF: 1 ETH @ $3000 (LT 82.5%) + 2000 USDC @ $1
    /// (LT 80%) vs 1500 USDC debt @ $1.
    /// numerator = 1 × 3000 × 0.825 + 2000 × 1 × 0.80 = 2475 + 1600 = 4075
    /// denominator = 1500
    /// HF = 4075 / 1500 ≈ 2.7166666666...
    #[test]
    fn hf_multi_asset_weighted() {
        let usdc = usdc_ref();
        let weth = weth_ref();
        // Use 1e6 scaling consistently to stay in i128 range: 1 ETH × 1e18 is
        // too large for trivial validation; instead scale all amounts so the
        // ratio is preserved.
        let account = mock_account(
            vec![
                (weth.clone(), U256::from(1_u64)), // 1 ETH unit
                (usdc.clone(), U256::from(2_000_u64)),
            ],
            vec![(usdc.clone(), U256::from(1_500_u64), RateMode::Variable)],
        );
        let collat_prices = vec![
            (weth.clone(), Decimal::new("3000")),
            (usdc.clone(), Decimal::new("1")),
        ];
        let debt_prices = vec![(usdc.clone(), Decimal::new("1"))];
        let lt = vec![(weth, 8_250_u32), (usdc, 8_000_u32)];

        let hf = recompute_health_factor(
            &account,
            &collat_prices,
            &debt_prices,
            &lt,
            Time::from_unix(0),
        )
        .unwrap();
        // 4075 / 1500 = 2.716666...
        // rust_decimal default precision: starts with full 28-digit precision.
        assert!(
            hf.as_str().starts_with("2.7166"),
            "expected ~2.7166..., got {}",
            hf.as_str()
        );
    }

    // -- LTV ------------------------------------------------------------

    /// PDF §11 fixture #6 inverse: debt $800 / collateral $2400 = 0.333...
    #[test]
    fn ltv_aave_v3_borrow_fixture() {
        let usdc = usdc_ref();
        let account = mock_account(
            vec![(usdc.clone(), U256::from(2_400_000_000_u64))],
            vec![(
                usdc.clone(),
                U256::from(800_000_000_u64),
                RateMode::Variable,
            )],
        );
        let prices = vec![(usdc, Decimal::new("1"))];

        let ltv = recompute_ltv(&account, &prices, &prices, Time::from_unix(0)).unwrap();
        // 800 / 2400 = 0.3333... — verify prefix instead of exact.
        assert!(
            ltv.as_str().starts_with("0.3333"),
            "expected ~0.3333..., got {}",
            ltv.as_str()
        );
    }

    #[test]
    fn ltv_zero_debt_is_zero() {
        let usdc = usdc_ref();
        let account = mock_account(vec![(usdc.clone(), U256::from(1_000_u64))], vec![]);
        let prices = vec![(usdc, Decimal::new("1"))];

        let ltv = recompute_ltv(&account, &prices, &prices, Time::from_unix(0)).unwrap();
        assert_eq!(ltv.as_str(), "0");
    }

    #[test]
    fn ltv_debt_with_zero_collateral_errors() {
        let usdc = usdc_ref();
        let account = mock_account(
            vec![],
            vec![(usdc.clone(), U256::from(100_u64), RateMode::Variable)],
        );
        let prices = vec![(usdc, Decimal::new("1"))];

        let err = recompute_ltv(&account, &prices, &prices, Time::from_unix(0)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // -- Perp PnL -------------------------------------------------------

    fn mock_perp(side: PerpSide, size_base: U256, entry: &str) -> PerpPosition {
        PerpPosition {
            venue: VenueRef::new("hyperliquid"),
            market: MarketRef {
                symbol: "ETH-USD".into(),
                venue: VenueRef::new("hyperliquid"),
            },
            side,
            size_base,
            notional_usd: U256::ZERO,
            collateral: vec![],
            entry_price: Decimal::new(entry),
            margin_mode: MarginMode::Cross,
            mark_price: dummy_live(Decimal::new("0")),
            liq_price: dummy_live(None),
            unrealized_pnl: dummy_live(SignedI256::ZERO),
            funding_owed: dummy_live(SignedI256::ZERO),
            leverage: dummy_live(Decimal::new("1")),
        }
    }

    /// PDF §11 fixture #7: Hyperliquid open long ETH 5x @ entry 3000.
    /// 2 ETH `size_base`, mark moves to 3100 → pnl = 2 × (3100 - 3000) = 200.
    #[test]
    fn pnl_long_eth_profit() {
        let pos = mock_perp(PerpSide::Long, U256::from(2_u64), "3000");
        let pnl = recompute_perp_pnl(&pos, &Decimal::new("3100"), Time::from_unix(0)).unwrap();
        assert_eq!(pnl, SignedI256::try_from(200_i32).unwrap());
    }

    /// Same fixture but mark falls to 2900 → loss.
    /// pnl = 2 × (2900 - 3000) = -200.
    #[test]
    fn pnl_long_eth_loss() {
        let pos = mock_perp(PerpSide::Long, U256::from(2_u64), "3000");
        let pnl = recompute_perp_pnl(&pos, &Decimal::new("2900"), Time::from_unix(0)).unwrap();
        assert_eq!(pnl, SignedI256::try_from(-200_i32).unwrap());
    }

    /// Short `PnL` flips the sign — mark falls = profit for short.
    /// 3 ETH short @ 3000, mark = 2800 → pnl = 3 × (3000 - 2800) = 600.
    #[test]
    fn pnl_short_eth_profit_on_drop() {
        let pos = mock_perp(PerpSide::Short, U256::from(3_u64), "3000");
        let pnl = recompute_perp_pnl(&pos, &Decimal::new("2800"), Time::from_unix(0)).unwrap();
        assert_eq!(pnl, SignedI256::try_from(600_i32).unwrap());
    }

    // -- Perp liq_price -------------------------------------------------

    /// Long 2 ETH @ 3000 with 1000 collateral (USD-treated): buffer = 1000/2
    /// = 500 → liq = 3000 - 500 = 2500.
    #[test]
    fn liq_price_long_simple() {
        let mut pos = mock_perp(PerpSide::Long, U256::from(2_u64), "3000");
        pos.collateral = vec![(usdc_ref(), U256::from(1_000_u64))];
        let liq = recompute_liq_price(&pos, Time::from_unix(0)).unwrap();
        assert_eq!(liq.unwrap().as_str(), "2500");
    }

    /// Short 2 ETH @ 3000 with 1000 collateral: liq = 3000 + 500 = 3500.
    #[test]
    fn liq_price_short_simple() {
        let mut pos = mock_perp(PerpSide::Short, U256::from(2_u64), "3000");
        pos.collateral = vec![(usdc_ref(), U256::from(1_000_u64))];
        let liq = recompute_liq_price(&pos, Time::from_unix(0)).unwrap();
        assert_eq!(liq.unwrap().as_str(), "3500");
    }

    /// Zero `size_base` → no position → None (matches closed-slot semantics).
    #[test]
    fn liq_price_zero_size_returns_none() {
        let pos = mock_perp(PerpSide::Long, U256::ZERO, "3000");
        let liq = recompute_liq_price(&pos, Time::from_unix(0)).unwrap();
        assert!(liq.is_none());
    }

    /// Long with overwhelming collateral clips at zero (liq cannot go neg).
    /// 1 ETH @ 3000, collateral 10000 → buffer = 10000 → liq = -7000 → 0.
    #[test]
    fn liq_price_long_negative_clips_to_zero() {
        let mut pos = mock_perp(PerpSide::Long, U256::from(1_u64), "3000");
        pos.collateral = vec![(usdc_ref(), U256::from(10_000_u64))];
        let liq = recompute_liq_price(&pos, Time::from_unix(0)).unwrap();
        assert_eq!(liq.unwrap().as_str(), "0");
    }

    // -- Round-trip sanity --------------------------------------------

    #[test]
    fn decimal_round_trip_through_rust_decimal() {
        let input = Decimal::new("1.0001");
        let parsed = parse_decimal(&input).unwrap();
        let back = decimal_to_state(parsed);
        assert_eq!(back.as_str(), "1.0001");
    }

    /// Sanity for `u256_to_decimal` — must not lose precision on small
    /// values that fit comfortably in i128.
    #[test]
    fn u256_to_decimal_small_amount() {
        let amount = U256::from(1_234_567_890_u64);
        let d = u256_to_decimal(amount).unwrap();
        assert_eq!(d.to_u64().unwrap(), 1_234_567_890_u64);
    }
}
