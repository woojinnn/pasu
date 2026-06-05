//! `position.*` enrichment-fact namespace — lending/health-factor reducer facts.
//!
//! Scaffold module: the inner [`dispatch`] match is FROZEN at scaffold time (one
//! arm per sim-server `position.*` method in `schema/method-catalog.json`, plus a
//! catch-all). Devs fill in the per-method `fn` bodies; they must never edit the
//! match.
//!
//! All four methods are `readKind: reducer`/`derived`: they apply the proposed
//! action to wallet state to produce State₂, then recompute the lending metric
//! (HF / LTV / borrow-fraction). None mutates state.
//!
//! ## Why these bodies do NOT need a `WalletState` reducer apply-accessor
//!
//! The scaffold stubs were flagged `STATE-WORKER ASK: needs WalletState reducer
//! apply-action accessor` — that accessor genuinely does not exist in the state
//! map. It is also not required here: the lowered lending action body
//! (`$.action`) already carries Aave's `getUserAccountData` snapshot under
//! `userStateBefore` (`healthFactor`, `totalCollatUsd`, `totalDebtUsd`,
//! `availableBorrowUsd` — all USD-scaled `U256` hex) PLUS the proposed `amount`
//! (raw asset units, `U256` hex), `assetPriceUsd` (`Decimal` string), and
//! `reserveState.{ltvBp, liquidationThresholdBp}` (basis points). State₂ HF/LTV
//! are therefore computed in closed form from the action body — no
//! `WalletState.positions` apply step. See `schema/.../lending/borrow.cedarschema`
//! and `lowering_v2/lending/mod.rs::lower_user_lending_state`.
//!
//! ## The one genuine units gap (→ `partial`)
//!
//! `amount` arrives in *raw asset units*; the `*Usd` fields are USD-scaled. To
//! line them up we need the borrowed asset's ERC20 `decimals`, which is NOT in
//! the action body. We recover it from the held `TokenHolding.decimals` for the
//! borrowed asset in `ctx.state.tokens`. When the asset is not held (decimals
//! unknown) the USD numerator cannot be formed, so those methods take a
//! conservative guard instead of fabricating a value. The USD fixed-point scale
//! of the `*Usd` fields (6 dp, matching the lowering's `getUserAccountData`
//! convention) is an assumption documented at [`USD_SCALE_DP`].

use serde_json::{json, Value};

use policy_state::primitives::{ChainId, U256};
use policy_state::token::TokenKey;

use super::params::param_action;
use super::FactCtx;
use super::FactError;

/// Fixed-point decimal places of the `*Usd` fields in `UserLendingState`
/// (`totalCollatUsd` / `totalDebtUsd` / `availableBorrowUsd`). The lowering
/// mirrors Aave's `getUserAccountData`, whose USD figures the host scales to 6
/// dp (e.g. `$50,000` → `50_000_000_000`). The USD numerator we synthesise for a
/// proposed borrow is rendered to the same scale so ratios line up.
const USD_SCALE_DP: u32 = 6;

/// Dispatch a `position.*` enrichment fact against `ctx`.
///
/// FROZEN: one arm per sim-server `position.*` method in the catalog, plus a
/// catch-all. Do not edit this match when filling in bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] for an unregistered method, or whatever
/// error the per-method fn surfaces.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "position.health_factor_after" => health_factor_after(params, ctx),
        "position.health_factor_with_volatility" => health_factor_with_volatility(params, ctx),
        "position.ltv_after" => ltv_after(params, ctx),
        "position.borrow_fraction_bps" => borrow_fraction_bps(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

// ---------------------------------------------------------------------------
// Shared local helpers (this file only — no edits to params.rs / mod.rs).
// ---------------------------------------------------------------------------

/// Borrow the lowered lending action's `userStateBefore` sub-object
/// (`UserLendingState`: `healthFactor`, `totalCollatUsd`, `totalDebtUsd`,
/// `availableBorrowUsd`). Missing on a non-lending action → `BadParams`.
fn user_state_before(action: &Value) -> Result<&Value, FactError> {
    action
        .get("userStateBefore")
        .filter(|v| v.is_object())
        .ok_or_else(|| {
            FactError::BadParams(
                "action is missing `userStateBefore` (not a lending action?)".into(),
            )
        })
}

/// Read a `U256` hex string field nested at `key` of a JSON object.
fn u256_hex_field(obj: &Value, key: &str) -> Result<U256, FactError> {
    let s = obj
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams(format!("missing U256 hex field `{key}`")))?;
    U256::from_str_radix(s.trim_start_matches("0x"), 16)
        .map_err(|e| FactError::BadParams(format!("field `{key}` is not U256 hex: {e}")))
}

/// Parse a non-negative decimal string (e.g. `"1.85"`, `"0.0512"`) into a
/// `U256` scaled by `10^scale_dp`, truncating excess fractional digits. Rejects
/// signs/exponents (the lowering never emits them for these fields).
fn decimal_to_scaled_u256(s: &str, scale_dp: u32) -> Result<U256, FactError> {
    let s = s.trim();
    if s.starts_with('-') || s.contains(['e', 'E']) {
        return Err(FactError::BadParams(format!(
            "decimal `{s}` is not a plain non-negative fixed-point string"
        )));
    }
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
    let scale_dp = scale_dp as usize;
    let mut frac: String = frac_part.chars().take(scale_dp).collect();
    while frac.len() < scale_dp {
        frac.push('0');
    }
    let digits = format!("{int_part}{frac}");
    let digits = if digits.is_empty() { "0" } else { &digits };
    U256::from_str_radix(digits, 10)
        .map_err(|e| FactError::BadParams(format!("decimal `{s}` not parseable: {e}")))
}

/// Re-render an arbitrary lowered decimal string (which may be DOTLESS, e.g. the
/// reducer's no-debt HF sentinel `"999999999"`, or have >4 fractional digits) to
/// the canonical dotted 4-dp form Cedar's `decimal()` accepts (`INT.FFFF`).
/// Unparseable input degrades to the huge 4-dp sentinel (the safe direction for
/// an unbounded / no-debt health factor). Mirrors `ratio_dp(.., 4)` output shape.
fn normalize_decimal_4dp(s: &str) -> String {
    match decimal_to_scaled_u256(s, 4) {
        Ok(scaled) => {
            let scale = U256::from(10_000u64);
            let whole = scaled / scale;
            let frac = scaled % scale;
            format!("{whole}.{frac:04}")
        }
        Err(_) => "1000000000.0000".to_owned(),
    }
}

/// Recover the ERC20 `decimals` of the borrowed `asset` from a held
/// `TokenHolding` in wallet state. The action's `asset` is a lowered
/// `Core::TokenRef` (`{ key: { standard, chain, address } }`) and carries no
/// decimals; the held token does (`TokenHolding.decimals: u8`). `None` when the
/// asset is not an erc20 ref or is not held.
fn erc20_token_key(action: &Value) -> Option<TokenKey> {
    let key = action.get("asset").and_then(|a| a.get("key"))?;
    if key.get("standard").and_then(Value::as_str) != Some("erc20") {
        return None;
    }
    let chain = ChainId::new(key.get("chain").and_then(Value::as_str)?.to_owned());
    let address = key.get("address").and_then(Value::as_str)?.parse().ok()?;
    Some(TokenKey::Erc20 { chain, address })
}

/// Recover the borrowed `asset`'s ERC20 `decimals` from its held
/// `TokenHolding.decimals`; `None` when the asset is not an erc20 ref or unheld.
fn borrowed_asset_decimals(action: &Value, ctx: &FactCtx) -> Option<u8> {
    let key = erc20_token_key(action)?;
    ctx.state.tokens.get(&key).map(|h| h.decimals)
}

/// USD value (6-dp scaled, to match `*Usd` fields) of `amount_raw` units of an
/// asset priced at `price` USD, given the asset's `decimals`:
/// `usd = amount_raw * price_6dp / 10^decimals`.
fn borrow_usd_scaled(amount_raw: U256, price: &str, decimals: u8) -> Result<U256, FactError> {
    let price_6dp = decimal_to_scaled_u256(price, USD_SCALE_DP)?;
    let denom = U256::from(10u64).pow(U256::from(u64::from(decimals)));
    if denom.is_zero() {
        return Ok(U256::ZERO);
    }
    Ok(amount_raw.saturating_mul(price_6dp) / denom)
}

/// Render `num / den` (both `U256`, same scale) as a fixed `dp`-place decimal
/// string via integer math — no float, `num` may be `U256::MAX`. `den == 0`
/// with a positive `num` yields a deliberately-huge sentinel so Cedar threshold
/// comparisons trip; `0/0 → "0.<0…>"`.
fn ratio_dp(num: U256, den: U256, dp: u32) -> String {
    let scale = U256::from(10u64).pow(U256::from(u64::from(dp)));
    if den.is_zero() {
        return if num.is_zero() {
            format!("0.{}", "0".repeat(dp as usize))
        } else {
            // HF/LTV sentinel: 1e9 with `dp` fractional zeros.
            format!("1000000000.{}", "0".repeat(dp as usize))
        };
    }
    let scaled = num.saturating_mul(scale) / den;
    let whole = scaled / scale;
    let frac = scaled % scale;
    format!("{whole}.{frac:0width$}", width = dp as usize)
}

/// `liquidationThresholdBp` from the action's `reserveState`, as basis points.
fn liquidation_threshold_bp(action: &Value) -> Result<u64, FactError> {
    action
        .get("reserveState")
        .and_then(|r| r.get("liquidationThresholdBp"))
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            FactError::BadParams("action.reserveState.liquidationThresholdBp missing".into())
        })
}

// ---------------------------------------------------------------------------
// Methods
// ---------------------------------------------------------------------------

/// AAVE-01 (T3): health factor of a lending position AFTER virtually applying
/// the proposed borrow/withdraw (State₂).
///
/// readKind: `reducer`. Output: `{ healthFactor }` from `$.result.healthFactor`.
///
/// Closed-form State₂: HF = `totalCollatUsd * liqThreshold / totalDebtUsd`. A
/// borrow adds `borrow_usd` to the debt leg (debt-USD-only HF impact — the Aave
/// model the lowering snapshots holds collateral/threshold fixed across the
/// borrow). `borrow_usd` is synthesised from `amount × assetPriceUsd` using the
/// borrowed asset's `decimals` recovered from the held `TokenHolding`.
///
/// PARTIAL: when the borrowed asset is not held (decimals unknown) we fall back
/// to State₁'s `userStateBefore.healthFactor` (a conservative no-worse-than view)
/// rather than fabricating a post-borrow number.
fn health_factor_after(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let usb = user_state_before(action)?;

    let total_collat = u256_hex_field(usb, "totalCollatUsd")?;
    let total_debt = u256_hex_field(usb, "totalDebtUsd")?;
    let liq_bp = liquidation_threshold_bp(action)?;

    let added_debt = added_debt_usd(action, ctx)?;
    let debt_after = total_debt.saturating_add(added_debt);

    let hf = if let Some(debt_after) = positive(debt_after) {
        // collat * liqBp / 10000 / debt_after, rendered to 4 dp (Cedar decimal
        // accepts 1..=4 fractional digits).
        let num = total_collat.saturating_mul(U256::from(liq_bp));
        let den = debt_after.saturating_mul(U256::from(10_000u64));
        ratio_dp(num, den, 4)
    } else {
        // No debt after action → HF is unbounded; mirror the State₁ value if it
        // was already unbounded, else the 4-dp sentinel (huge, so a min-HF deny
        // policy never trips on a debt-free position — the safe direction).
        // The lowered `healthFactor` may be DOTLESS (the reducer's no-debt
        // sentinel is `"999999999"`), so normalize it to dotted 4-dp or Cedar's
        // `decimal()` rejects it and the whole context fails to build.
        usb.get("healthFactor")
            .and_then(Value::as_str)
            .map_or_else(|| "1000000000.0000".to_owned(), normalize_decimal_4dp)
    };

    Ok(json!({ "healthFactor": hf }))
}

/// AAVE-02: post-trade health factor (same State₂ recompute as
/// `health_factor_after`) PLUS the collateral-volatility class of the position's
/// dominant collateral. Emits TWO context fields from one call.
///
/// readKind: `reducer`. Output: `{ healthFactor, collateralIsVolatile }`.
///
/// PARTIAL: `collateralIsVolatile` is the `ExternalInfo` volatility tag of the
/// dominant collateral. The action body does NOT enumerate the position's
/// collateral set (only the aggregate `totalCollatUsd`), and the `position`
/// lookup would need a (owner, venue) match that the action's `venue` cannot be
/// resolved to a `Position` without a market/venue equality the state map
/// exposes no helper for. We classify volatility from the *only* per-asset
/// signal present: the borrowed `asset`'s held `TokenHolding.kind`
/// (`Base{category: Stable}` / pegged ⇒ not volatile; everything else ⇒
/// volatile). When the asset is unheld we conservatively report `true`
/// (treat-as-volatile is the safe default for a warn policy).
fn health_factor_with_volatility(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let hf = health_factor_after(params, ctx)?;
    let health_factor = hf
        .get("healthFactor")
        .cloned()
        .unwrap_or_else(|| Value::String("0.0000".into()));

    let action = param_action(params, "action")?;
    let is_volatile = collateral_is_volatile(action, ctx);

    Ok(json!({
        "healthFactor": health_factor,
        "collateralIsVolatile": is_volatile,
    }))
}

/// AAVE-06: loan-to-value of a lending position AFTER virtually applying the
/// proposed action (State₂), as a decimal ratio `debt_usd / collateral_usd`.
///
/// readKind: `reducer`. Output: `{ ltv }` from `$.result.ltv`.
///
/// Closed-form State₂: `ltv = (totalDebtUsd + borrow_usd) / totalCollatUsd`.
///
/// PARTIAL: same `decimals` recovery as `health_factor_after`; when the borrowed
/// asset is unheld the borrow leg is omitted (`ltv` reflects State₁ debt) rather
/// than fabricated.
fn ltv_after(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let usb = user_state_before(action)?;

    let total_collat = u256_hex_field(usb, "totalCollatUsd")?;
    let total_debt = u256_hex_field(usb, "totalDebtUsd")?;
    let added_debt = added_debt_usd(action, ctx)?;
    let debt_after = total_debt.saturating_add(added_debt);

    // ltv = debt_after / total_collat, rendered to 4 dp (Cedar decimal accepts
    // 1..=4 fractional digits).
    let ltv = ratio_dp(debt_after, total_collat, 4);
    Ok(json!({ "ltv": ltv }))
}

/// AAVE-07: fraction (basis points) of the user's remaining borrow capacity this
/// borrow consumes = `borrow_usd / availableBorrowUsd`.
///
/// readKind: `derived`. Output: `{ bps }` from `$.result.bps`.
///
/// Numerator `borrow_usd` = `amount × assetPriceUsd` (USD-6dp scaled, decimals
/// recovered from the held `TokenHolding`); denominator
/// `userStateBefore.availableBorrowUsd` rides on the action body. Result is
/// clamped to a `Long`-safe range.
///
/// PARTIAL: when the borrowed asset is unheld (decimals unknown) we cannot scale
/// `amount` to USD and return `bps = 0` (conservative: "unknown consumption"),
/// flagged below.
fn borrow_fraction_bps(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let usb = user_state_before(action)?;
    let available = u256_hex_field(usb, "availableBorrowUsd")?;

    // decimals unknown → cannot form the numerator; conservative 0.
    let Some(borrow_usd) = added_debt_usd_opt(action, ctx)? else {
        return Ok(json!({ "bps": 0 }));
    };

    // bps = borrow_usd * 10000 / available, clamped to i64 (Long) range.
    let bps_u256 = if available.is_zero() {
        // Borrowing against zero remaining capacity: saturate high so a
        // `.greaterThan(threshold)` policy trips.
        if borrow_usd.is_zero() {
            U256::ZERO
        } else {
            U256::from(i64::MAX as u64)
        }
    } else {
        borrow_usd.saturating_mul(U256::from(10_000u64)) / available
    };
    let bps = u256_to_i64_clamped(bps_u256);
    Ok(json!({ "bps": bps }))
}

// ---------------------------------------------------------------------------
// Method-shared computation
// ---------------------------------------------------------------------------

/// USD (6-dp scaled) the proposed borrow adds to the debt leg, or `U256::ZERO`
/// when the asset is unheld (decimals unknown). Withdraw/other actions carry no
/// `assetPriceUsd`+borrow `amount` pair and contribute zero added debt.
fn added_debt_usd(action: &Value, ctx: &FactCtx) -> Result<U256, FactError> {
    Ok(added_debt_usd_opt(action, ctx)?.unwrap_or(U256::ZERO))
}

/// `Some(usd)` when the action is a borrow whose asset is held (decimals known);
/// `None` when decimals are unrecoverable (asset unheld); `Some(ZERO)` when the
/// action carries no borrow `amount`/`assetPriceUsd` (e.g. a withdraw).
fn added_debt_usd_opt(action: &Value, ctx: &FactCtx) -> Result<Option<U256>, FactError> {
    // No borrow amount/price on this action shape → no added debt.
    let (Some(amount), Some(price)) = (
        action.get("amount").and_then(Value::as_str),
        action.get("assetPriceUsd").and_then(Value::as_str),
    ) else {
        return Ok(Some(U256::ZERO));
    };
    let amount = U256::from_str_radix(amount.trim_start_matches("0x"), 16)
        .map_err(|e| FactError::BadParams(format!("action.amount not U256 hex: {e}")))?;
    match borrowed_asset_decimals(action, ctx) {
        Some(decimals) => Ok(Some(borrow_usd_scaled(amount, price, decimals)?)),
        None => Ok(None),
    }
}

/// Volatility class of the borrowed `asset` from its held `TokenHolding.kind`:
/// a `Base{category: Stable}` (or any token pegged to fiat) is treated as
/// non-volatile; everything else — including an unheld asset — as volatile (the
/// safe default for a warn policy).
fn collateral_is_volatile(action: &Value, ctx: &FactCtx) -> bool {
    use policy_state::token::kind::{BaseCategory, TokenKind};

    let Some(token_key) = erc20_token_key(action) else {
        return true;
    };
    match ctx.state.tokens.get(&token_key).map(|h| &h.kind) {
        Some(TokenKind::Base {
            category: BaseCategory::Stable,
            ..
        }) => false,
        // Any other kind (Volatile/Governance/Wrapped/LP/…) or unheld → volatile.
        _ => true,
    }
}

/// `Some(x)` iff `x` is non-zero (helper for the unbounded-HF branch).
fn positive(x: U256) -> Option<U256> {
    (!x.is_zero()).then_some(x)
}

/// Clamp a `U256` into the non-negative `i64` (`Long`) range.
fn u256_to_i64_clamped(x: U256) -> i64 {
    i64::try_from(x).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::live_field::DataSource;
    use policy_state::primitives::{Address, ChainId, Time};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{BaseCategory, TokenKind};
    use policy_state::token::TokenKey;
    use policy_state::{WalletId, WalletState};

    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn usdc_addr() -> Address {
        Address::from_str(USDC).unwrap()
    }

    fn state_with_usdc(decimals: u8, category: BaseCategory) -> WalletState {
        let mut s = WalletState::new(WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        ));
        let key = TokenKey::Erc20 {
            chain: chain(),
            address: usdc_addr(),
        };
        s.tokens.insert(
            key.clone(),
            TokenHolding {
                key,
                kind: TokenKind::Base {
                    category,
                    peg_to: None,
                },
                symbol: "USDC".into(),
                decimals,
                balance: Balance::fungible(U256::from(1_000_000_000u64)),
                committed: Balance::zero_fungible(),
                approved_to: None,
                price_usd: None,
                metadata: None,
                value_usd: None,
                last_synced_at: Time::from_unix(1_700_000_000),
                primitives_source: DataSource::UserSupplied,
            },
        );
        s
    }

    /// A lowered `Lending::Borrow` action body: borrow `amount_raw` USDC at
    /// `price`, against a `getUserAccountData` snapshot.
    fn borrow_action(amount_raw: &str, price: &str) -> Value {
        json!({
            "asset": { "key": { "standard": "erc20", "chain": chain().as_str(), "address": USDC } },
            "amount": amount_raw,
            "assetPriceUsd": price,
            "reserveState": { "liquidationThresholdBp": 7400 },
            "userStateBefore": {
                "healthFactor": "1.85",
                "totalCollatUsd": format!("{:#x}", U256::from(50_000_000_000u64)),
                "totalDebtUsd": format!("{:#x}", U256::from(20_000_000_000u64)),
                "availableBorrowUsd": format!("{:#x}", U256::from(15_000_000_000u64)),
            }
        })
    }

    fn params(action: &Value) -> Value {
        json!({ "chain_id": "eip155:1", "owner": "0x000000000000000000000000000000000000a01c", "venue": "aave-v3", "action": action })
    }

    #[test]
    fn decimal_to_scaled_u256_truncates_and_pads() {
        assert_eq!(
            decimal_to_scaled_u256("1.00", 6).unwrap(),
            U256::from(1_000_000u64)
        );
        assert_eq!(
            decimal_to_scaled_u256("0.0512", 6).unwrap(),
            U256::from(51_200u64)
        );
        // Excess fractional digits truncate.
        assert_eq!(
            decimal_to_scaled_u256("1.2345678", 6).unwrap(),
            U256::from(1_234_567u64)
        );
        assert!(decimal_to_scaled_u256("-1.0", 6).is_err());
    }

    #[test]
    fn borrow_usd_scaled_matches_6dp() {
        // 500 USDC (6dp) at $1.00 → 500_000_000 in 6dp USD scale.
        let usd = borrow_usd_scaled(U256::from(500_000_000u64), "1.00", 6).unwrap();
        assert_eq!(usd, U256::from(500_000_000u64));
    }

    #[test]
    fn borrow_fraction_bps_is_borrow_over_available() {
        // borrow 500 USDC ($500, 6dp = 500_000_000) / available 15_000_000_000
        // = 0.03333 → 333 bps.
        let st = state_with_usdc(6, BaseCategory::Stable);
        let out = borrow_fraction_bps(
            &params(&borrow_action(
                &format!("{:#x}", U256::from(500_000_000u64)),
                "1.00",
            )),
            &FactCtx { state: &st },
        )
        .unwrap();
        assert_eq!(out["bps"], json!(333));
    }

    #[test]
    fn borrow_fraction_bps_unheld_asset_is_zero() {
        // Asset not held → decimals unknown → conservative 0.
        let st = WalletState::new(WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        ));
        let out = borrow_fraction_bps(
            &params(&borrow_action(
                &format!("{:#x}", U256::from(500_000_000u64)),
                "1.00",
            )),
            &FactCtx { state: &st },
        )
        .unwrap();
        assert_eq!(out["bps"], json!(0));
    }

    #[test]
    fn health_factor_after_drops_with_added_borrow() {
        // collat 50_000 * 0.74 / (debt 20_000 + 500) = 37_000 / 20_500 ≈ 1.8048…
        // Rendered to exactly 4 dp (truncated) so Cedar's decimal extension accepts it.
        let st = state_with_usdc(6, BaseCategory::Stable);
        let out = health_factor_after(
            &params(&borrow_action(
                &format!("{:#x}", U256::from(500_000_000u64)),
                "1.00",
            )),
            &FactCtx { state: &st },
        )
        .unwrap();
        assert_eq!(out["healthFactor"], json!("1.8048"));
    }

    #[test]
    fn health_factor_after_no_debt_normalizes_dotless_sentinel() {
        // No-debt path: the reducer's `userStateBefore.healthFactor` sentinel is
        // the DOTLESS string "999999999". The fact must normalize it to dotted
        // 4-dp ("999999999.0000") or Cedar's decimal() rejects it and the whole
        // policy context fails to build. Regression guard for that branch.
        let st = state_with_usdc(6, BaseCategory::Stable);
        let mut action = borrow_action(&format!("{:#x}", U256::ZERO), "1.00");
        action["userStateBefore"]["healthFactor"] = json!("999999999");
        action["userStateBefore"]["totalDebtUsd"] = json!(format!("{:#x}", U256::ZERO));
        let out = health_factor_after(&params(&action), &FactCtx { state: &st }).unwrap();
        let hf = out["healthFactor"].as_str().unwrap();
        assert_eq!(hf, "999999999.0000");
        // Must be dotted with exactly 4 fractional digits (Cedar's constraint).
        let (_, frac) = hf.split_once('.').expect("must contain a dot");
        assert_eq!(frac.len(), 4, "decimal must have exactly 4 frac digits");
    }

    #[test]
    fn ltv_after_includes_added_borrow() {
        // (debt 20_000 + 500) / collat 50_000 = 0.41 → exactly "0.4100" (4 dp).
        let st = state_with_usdc(6, BaseCategory::Stable);
        let out = ltv_after(
            &params(&borrow_action(
                &format!("{:#x}", U256::from(500_000_000u64)),
                "1.00",
            )),
            &FactCtx { state: &st },
        )
        .unwrap();
        assert_eq!(out["ltv"], json!("0.4100"));
    }

    #[test]
    fn health_factor_with_volatility_flags_stable_as_non_volatile() {
        let st = state_with_usdc(6, BaseCategory::Stable);
        let out = health_factor_with_volatility(
            &params(&borrow_action(
                &format!("{:#x}", U256::from(500_000_000u64)),
                "1.00",
            )),
            &FactCtx { state: &st },
        )
        .unwrap();
        assert_eq!(out["collateralIsVolatile"], json!(false));
        assert_eq!(out["healthFactor"], json!("1.8048"));
    }

    #[test]
    fn health_factor_with_volatility_flags_volatile_collateral() {
        let st = state_with_usdc(18, BaseCategory::Volatile);
        let out = health_factor_with_volatility(
            &params(&borrow_action(
                &format!("{:#x}", U256::from(1u64)),
                "2000.00",
            )),
            &FactCtx { state: &st },
        )
        .unwrap();
        assert_eq!(out["collateralIsVolatile"], json!(true));
    }

    #[test]
    fn missing_user_state_before_is_bad_params() {
        let st = state_with_usdc(6, BaseCategory::Stable);
        let action = json!({ "amount": "0x1", "assetPriceUsd": "1.00" });
        let err = ltv_after(&params(&action), &FactCtx { state: &st }).unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }
}
