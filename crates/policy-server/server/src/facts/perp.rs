//! `perp.*` enrichment-fact namespace (sim-server fact host).
//!
//! Mechanically generated scaffold mirroring the `perp.` sim-server entries of
//! `schema/method-catalog.json` (`planned`, `server == "sim-server"`). Ten
//! methods; one [`dispatch`] arm and one private `fn` each.
//!
//! The inner `match` in [`dispatch`] is FROZEN at scaffold time — it is the
//! conflict-free registry surface. Devs filling in a body must edit ONLY that
//! body fn, never the match.
//!
//! ## Param shape (lowered Cedar action body)
//!
//! `action` arrives as the lowered Cedar `Perp::*` context (see
//! `policy-engine/src/lowering_v2/perp/*`):
//!   - `live_inputs` is **flattened** onto the action object: `markPrice`,
//!     `fundingRate`, `userAccountState` are top-level keys (NOT under a
//!     `live_inputs.*` path — the catalog's `stateDependency` names them
//!     `live_inputs.*` only as a logical reference).
//!   - keys are **camelCase** (`reduceOnly`, `newMode`, `newLeverage`,
//!     `triggerPrice`, `orderKind`, `userAccountState.totalCollateralUsd`).
//!   - `size` is the discriminated `SizeSpec` object
//!     (`{ kind: "base_amount" | "quote_amount" | "leverage_implied", … }`).
//!   - U256 amounts are lowercase `0x`-hex strings; `Decimal`/`Price`/leverage/
//!     funding are plain decimal strings; `side` is `"long"`/`"short"`,
//!     `margin_mode`/`newMode` are `"cross"`/`"isolated"`.
//!
//! `WalletState::positions` is a `Vec<Position>`; perp positions are matched by
//! navigating `position.kind == PositionKind::PerpPosition(p)` and comparing the
//! `market` param against `p.market.symbol` / `position.id`. No typed perp getter
//! exists on `WalletState`, so the match is done inline.

use serde_json::{json, Value};

use policy_state::position::{MarginMode, PerpPosition, PerpSide, PositionKind};
use policy_state::primitives::U256;

use super::params::{over_balance_4dp, param_action, param_str};
use super::FactCtx;
use super::FactError;

/// Dispatch a `perp.*` enrichment fact by `method` name.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] for a name outside this namespace, and
/// [`FactError::NotImplemented`] for a registered-but-unfilled stub.
///
/// FROZEN: one arm per `perp.*` sim-server catalog method. Do not edit — fill in
/// the body fns instead.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "perp.notional_to_collateral" => perp_notional_to_collateral(params, ctx),
        "perp.position_state" => perp_position_state(params, ctx),
        "perp.liq_distance_bp" => perp_liq_distance_bp(params, ctx),
        "perp.order_leverage" => perp_order_leverage(params, ctx),
        "perp.funding_adverse_rate" => perp_funding_adverse_rate(params, ctx),
        "perp.margin_mode_transition" => perp_margin_mode_transition(params, ctx),
        "perp.crosses_zero" => perp_crosses_zero(params, ctx),
        "perp.cross_total_exposure_ratio" => perp_cross_total_exposure_ratio(params, ctx),
        "perp.leverage_increase" => perp_leverage_increase(params, ctx),
        "perp.stop_trigger_misplaced" => perp_stop_trigger_misplaced(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

// ---------------------------------------------------------------------------
// Local helpers (inline; this file may not edit params.rs — concurrency rule).
// ---------------------------------------------------------------------------

/// Decimal-string fixed-point scale (1e9). Wide enough for prices, leverage,
/// funding rates, and ratios at the precision the policies compare against.
const DEC_SCALE: i128 = 1_000_000_000;

/// Parse a `Decimal`/`Price` string (e.g. `"3050"`, `"0.0001"`, `"-7"`) into an
/// `i128` scaled by [`DEC_SCALE`]. `None` on a non-numeric/over-wide string —
/// callers treat that as "unknown" and degrade conservatively rather than panic.
///
/// `Decimal` is a `String` newtype with no arithmetic ops (state-map rule), so
/// every numeric comparison in this file routes through this fixed-point parse.
fn parse_decimal_scaled(s: &str) -> Option<i128> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
        || (int_part.is_empty() && frac_part.is_empty())
    {
        return None;
    }
    let int_val: i128 = if int_part.is_empty() {
        0
    } else {
        int_part.parse().ok()?
    };
    // Take/pad the fractional part to exactly 9 digits, truncating extra.
    let mut frac9 = String::with_capacity(9);
    for c in frac_part.chars().take(9) {
        frac9.push(c);
    }
    while frac9.len() < 9 {
        frac9.push('0');
    }
    let frac_val: i128 = frac9.parse().ok()?;
    let scaled = int_val.checked_mul(DEC_SCALE)?.checked_add(frac_val)?;
    Some(if neg { -scaled } else { scaled })
}

/// Render a [`DEC_SCALE`]-scaled `i128` to a dotted 4-decimal-place string
/// (`INT.FFFF`, exactly 4 fractional digits — never dotless). Cedar's `decimal`
/// extension rejects both dotless integers (`"5"`) and >4 fractional digits, so
/// every `decimal`-typed output in this file must route through here.
fn render_scaled_4dp(v: i128) -> String {
    let neg = v < 0;
    let v = v.unsigned_abs();
    let whole = v / DEC_SCALE as u128;
    // Take the top 4 of the 9 fractional digits (truncate the trailing 5).
    let frac4 = (v % DEC_SCALE as u128) / 100_000;
    let body = format!("{whole}.{frac4:04}");
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// Parse a `Decimal`/leverage/funding string (dotless `"5"`, dotted `"12.5"`,
/// signed `"-0.0003"`) and re-render it as a dotted 4-dp string. Unparseable
/// input degrades to `"0.0000"` so a `decimal`-typed field is never emitted in a
/// Cedar-rejecting (dotless / over-wide) form.
fn to_4dp(s: &str) -> String {
    parse_decimal_scaled(s).map_or_else(|| "0.0000".to_owned(), render_scaled_4dp)
}

/// Read the lowered `side` field (`"long"`/`"short"`) off the action.
fn action_side(action: &Value) -> Option<&'static str> {
    match action.get("side").and_then(Value::as_str)? {
        "long" => Some("long"),
        "short" => Some("short"),
        _ => None,
    }
}

/// `side` spelling for a state-side [`PerpSide`].
const fn perp_side_str(side: &PerpSide) -> &'static str {
    match side {
        PerpSide::Long => "long",
        PerpSide::Short => "short",
    }
}

/// Borrow the lowered `market` identifier from the `action` body. Open/limit/stop
/// orders carry `market.symbol`; increase/decrease carry `positionId`.
fn action_market_ident(action: &Value) -> Option<String> {
    if let Some(sym) = action
        .get("market")
        .and_then(|m| m.get("symbol"))
        .and_then(Value::as_str)
    {
        return Some(sym.to_owned());
    }
    action
        .get("positionId")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Find the first perp `Position` whose `market.symbol` OR enclosing
/// `position.id` matches `ident`. Matching is lenient because the `market` param
/// is sometimes a market symbol (`$.action.market`) and sometimes a positionId
/// (`$.action.positionId`, per the catalog `sampleCall`).
fn find_perp<'a>(ctx: &'a FactCtx<'a>, ident: &str) -> Option<&'a PerpPosition> {
    ctx.state.positions.iter().find_map(|pos| {
        if let PositionKind::PerpPosition(p) = &pos.kind {
            if pos.id == ident || p.market.symbol == ident {
                return Some(p);
            }
        }
        None
    })
}

/// Read a `U256` from a lowered hex (`0x…`) field on `obj` by `key`.
fn u256_hex_field(obj: &Value, key: &str) -> Option<U256> {
    let s = obj.get(key).and_then(Value::as_str)?;
    U256::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// Pull the order's base size (raw integer units) out of the lowered `SizeSpec`.
/// Only the `base_amount` arm has a directly-comparable base size; the
/// `quote_amount` / `leverage_implied` arms carry USD/collateral instead and
/// return `None` here (callers degrade).
fn size_base_amount(action: &Value) -> Option<U256> {
    let size = action.get("size")?;
    match size.get("kind").and_then(Value::as_str)? {
        "base_amount" => u256_hex_field(size, "amount"),
        _ => None,
    }
}

/// Total collateral USD (raw integer) from the action's flattened
/// `userAccountState.totalCollateralUsd`, or `0` when absent.
fn action_collateral_usd(action: &Value) -> U256 {
    action
        .get("userAccountState")
        .and_then(|s| u256_hex_field(s, "totalCollateralUsd"))
        .unwrap_or(U256::ZERO)
}

// ---------------------------------------------------------------------------
// Methods.
// ---------------------------------------------------------------------------

/// `perp.notional_to_collateral` — PERP-03. Ratio of the proposed order's
/// notional (size × markPrice) to total collateral USD.
///
/// readKind: `derived`
///
/// outputs: `ratio`: decimal — `$.result.ratio`
///
/// PARTIAL: the `quote_amount` size arm gives notional directly in USD
/// (`amountUsd`), so its ratio is exact. The `base_amount` arm needs
/// `base × markPrice` in USD units, but the order's base-token decimals are not
/// in the lowered action (only the raw integer `size.amount`), so we cannot
/// rescale base units onto the USD collateral basis — that arm degrades to ratio
/// `"0"`. The cross-position collateral variant is not folded (the denominator is
/// the action's `userAccountState.totalCollateralUsd`).
fn perp_notional_to_collateral(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let collateral = action_collateral_usd(action);

    let size = action.get("size");
    let notional_usd = match size.and_then(|s| s.get("kind")).and_then(Value::as_str) {
        Some("quote_amount") => size
            .and_then(|s| u256_hex_field(s, "amountUsd"))
            .unwrap_or(U256::ZERO),
        // PARTIAL: base_amount notional needs base-token decimals (absent in the
        // lowered action) to convert to the USD collateral basis. Degrade to 0.
        _ => U256::ZERO,
    };

    Ok(json!({ "ratio": over_balance_4dp(notional_usd, collateral) }))
}

/// `perp.position_state` — PERP-08/09/11. Existing perp position's side /
/// effective leverage / unrealized-pnl % from durable wallet state.
///
/// readKind: `direct`
///
/// outputs: `side`: String — `$.result.side`; `leverage`: decimal —
/// `$.result.leverage`; `unrealizedPnlPct`: decimal — `$.result.unrealizedPnlPct`
///
/// PARTIAL: `side` and `leverage` are exact reads off the matched `PerpPosition`.
/// `unrealizedPnlPct = unrealized_pnl / notional_usd × 100` is computed in
/// integer math: `unrealized_pnl` is a signed integer (`SignedI256`) and
/// `notional_usd` a `U256`, both raw USD integer units carried on the position,
/// so the percent ratio is exact. When no position matches, all three fields are
/// emitted as neutral defaults (`side: ""`, `leverage: "0.0000"`,
/// `unrealizedPnlPct: "0.0000"`). The two `decimal`-typed outputs are always
/// rendered dotted 4dp so Cedar's `decimal` extension accepts them.
fn perp_position_state(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let market = param_str(params, "market")?;
    let Some(p) = find_perp(ctx, &market) else {
        // `decimal`-typed defaults must be dotted 4dp (never bare "0").
        return Ok(json!({ "side": "", "leverage": "0.0000", "unrealizedPnlPct": "0.0000" }));
    };

    let side = perp_side_str(&p.side);
    // `decimal`-typed output: re-render the state leverage to dotted 4dp.
    let leverage = to_4dp(p.leverage.value.as_str());

    // unrealizedPnlPct = pnl / notional × 100, integer math (no Decimal ops). pnl
    // is SignedI256; notional_usd is U256 — both raw integer USD units on state.
    let pnl = p.unrealized_pnl.value;
    let notional = p.notional_usd;
    let pnl_pct = if notional.is_zero() {
        "0.0000".to_owned()
    } else {
        let neg = pnl.is_negative();
        let mag: U256 = pnl.unsigned_abs();
        // (|pnl| * 1_000_000 / notional) → "<whole>.<frac4>" percent (dotted 4dp).
        let scaled = mag.saturating_mul(U256::from(1_000_000u64)) / notional;
        let whole = scaled / U256::from(10_000u64);
        let frac = scaled % U256::from(10_000u64);
        let s = format!("{whole}.{frac:04}");
        if neg {
            format!("-{s}")
        } else {
            s
        }
    };

    Ok(json!({ "side": side, "leverage": leverage, "unrealizedPnlPct": pnl_pct }))
}

/// `perp.liq_distance_bp` — PERP-02. Distance (bp) from liquidation price to mark
/// price for the position on the order's market.
///
/// readKind: `reducer`
///
/// outputs: `liqDistanceBp`: Long — `$.result.liqDistanceBp`
///
/// PARTIAL: the catalog asks for State₂ (distance AFTER opening, via a reducer).
/// No `apply(action) -> delta` is exposed to facts (state-map: "NO apply
/// function"), so we compute the State₁ distance from the EXISTING position's
/// `liq_price` against the action's live `markPrice`:
/// `bp = |markPrice − liqPrice| / markPrice × 10000`, in fixed-point. When no
/// matching position exists, or `liq_price` is `None`, we emit a large sentinel
/// (`10000` bp = 100% away = "safe"): there is nothing close to liquidating yet.
fn perp_liq_distance_bp(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let mark = action
        .get("markPrice")
        .and_then(Value::as_str)
        .and_then(parse_decimal_scaled);

    let liq = action_market_ident(action)
        .as_deref()
        .and_then(|id| find_perp(ctx, id))
        .and_then(|p| p.liq_price.value.as_ref())
        .and_then(|price| parse_decimal_scaled(price.as_str()));

    let bp = match (mark, liq) {
        (Some(m), Some(l)) if m > 0 => {
            let diff = (m - l).abs();
            // (diff / mark) * 10000, integer fixed-point (both already *1e9).
            i64::try_from(diff.saturating_mul(10_000) / m).unwrap_or(i64::MAX)
        }
        // No live position / no liq price → nothing close to liquidation (safe).
        _ => 10_000,
    };

    Ok(json!({ "liqDistanceBp": bp }))
}

/// `perp.order_leverage` — PERP-01. The proposed open's effective leverage as a
/// decimal (explicit `leverage` field, or the `leverage_implied` `SizeSpec` arm).
///
/// readKind: `derived` (no durable-state read)
///
/// outputs: `leverage`: decimal — `$.result.leverage`
fn perp_order_leverage(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;

    // Explicit `leverage` (OpenPosition) wins; else the leverage_implied size arm.
    let leverage = action
        .get("leverage")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            let size = action.get("size")?;
            if size.get("kind").and_then(Value::as_str) == Some("leverage_implied") {
                size.get("leverage")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "0".to_owned());

    // `decimal`-typed output: re-render to dotted 4dp ("5" -> "5.0000",
    // "12.5" -> "12.5000", default "0" -> "0.0000").
    Ok(json!({ "leverage": to_4dp(&leverage) }))
}

/// `perp.funding_adverse_rate` — PERP-04. Funding rate sign-aligned to the order
/// side, returned as the adverse magnitude (>0 = funding works against entry).
///
/// readKind: `derived` (no durable-state read)
///
/// outputs: `adverseRate`: decimal — `$.result.adverseRate`
///
/// Convention: a positive `fundingRate` is paid by longs to shorts. So for a
/// long the adverse magnitude is `+fundingRate` (when positive); for a short it
/// is `−fundingRate` (when funding is negative, i.e. shorts pay). A
/// favourable-funding side returns `"0.0000"`. Every branch is rendered dotted
/// 4dp (`INT.FFFF`) so Cedar's `decimal` extension accepts the arg.
fn perp_funding_adverse_rate(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let side = action_side(action);
    let rate = action
        .get("fundingRate")
        .and_then(Value::as_str)
        .and_then(parse_decimal_scaled);

    // `decimal`-typed output: every branch must be dotted 4dp (INT.FFFF).
    let adverse = match (side, rate) {
        // Long is adverse when funding is positive (longs pay).
        (Some("long"), Some(r)) if r > 0 => render_scaled_4dp(r),
        // Short is adverse when funding is negative (shorts pay); report magnitude.
        (Some("short"), Some(r)) if r < 0 => render_scaled_4dp(-r),
        _ => "0.0000".to_owned(),
    };

    Ok(json!({ "adverseRate": adverse }))
}

/// `perp.margin_mode_transition` — PERP-06. Is this a risky isolated→cross margin
/// switch? Compares the action's `newMode` against the current margin mode in
/// durable state.
///
/// readKind: `direct`
///
/// outputs: `isolatedToCross`: Bool — `$.result.isolatedToCross`
fn perp_margin_mode_transition(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let market = param_str(params, "market")?;
    let action = param_action(params, "action")?;
    let new_mode = action.get("newMode").and_then(Value::as_str);

    let current_isolated = matches!(
        find_perp(ctx, &market).map(|p| &p.margin_mode),
        Some(MarginMode::Isolated)
    );
    let isolated_to_cross = current_isolated && new_mode == Some("cross");

    Ok(json!({ "isolatedToCross": isolated_to_cross }))
}

/// `perp.crosses_zero` — PERP-08. Would this reduce/decrease order overshoot 0
/// and flip the position to the opposite side? Compares the order's decrease size
/// to the current position `size_base` in durable state.
///
/// readKind: `direct`
///
/// outputs: `crossesZero`: Bool — `$.result.crossesZero`
///
/// PARTIAL: a flip needs a decrease `size` strictly greater than the held
/// `size_base`. Only the `base_amount` size arm is directly comparable to
/// `size_base`; `quote_amount` / `leverage_implied` reductions carry
/// USD/collateral instead and conservatively return `false` (no false-positive
/// deny). When no position is held, `false`.
fn perp_crosses_zero(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let Some(p) = action_market_ident(action)
        .as_deref()
        .and_then(|id| find_perp(ctx, id))
    else {
        return Ok(json!({ "crossesZero": false }));
    };

    let crosses = match size_base_amount(action) {
        Some(dec) => dec > p.size_base,
        // PARTIAL: non-base size arm not comparable to size_base → conservative false.
        None => false,
    };

    Ok(json!({ "crossesZero": crosses }))
}

/// `perp.cross_total_exposure_ratio` — PERP-10. (Σ held cross-margin positions'
/// notional + the new order's notional) / account collateral.
///
/// readKind: `fold`
///
/// outputs: `ratio`: decimal — `$.result.ratio`
///
/// PARTIAL: folds every CROSS-margin perp `Position`'s `notional_usd` (a raw USD
/// integer already on the position — exact, no base×markPrice rescale needed) and
/// adds the new order's notional. The new order's notional is exact only for the
/// `quote_amount` size arm (`amountUsd`); other arms contribute 0 (same
/// base-decimals gap as `perp.notional_to_collateral`). The denominator is the
/// action's `userAccountState.totalCollateralUsd`.
fn perp_cross_total_exposure_ratio(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;

    // Σ notional_usd over CROSS-margin perp positions (exact integer USD on state).
    let mut total = U256::ZERO;
    for pos in &ctx.state.positions {
        if let PositionKind::PerpPosition(p) = &pos.kind {
            if matches!(p.margin_mode, MarginMode::Cross) {
                total = total.saturating_add(p.notional_usd);
            }
        }
    }

    // New order's notional: exact for quote_amount, else 0 (PARTIAL, see doc).
    let size = action.get("size");
    if size.and_then(|s| s.get("kind")).and_then(Value::as_str) == Some("quote_amount") {
        if let Some(n) = size.and_then(|s| u256_hex_field(s, "amountUsd")) {
            total = total.saturating_add(n);
        }
    }

    let collateral = action_collateral_usd(action);

    Ok(json!({ "ratio": over_balance_4dp(total, collateral) }))
}

/// `perp.leverage_increase` — PERP-11. Does this market have an OPEN position AND
/// is the requested `newLeverage` higher than the current leverage?
///
/// readKind: `direct`
///
/// outputs: `leverageIncrease`: Bool — `$.result.leverageIncrease`
fn perp_leverage_increase(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let market = param_str(params, "market")?;
    let action = param_action(params, "action")?;
    let new_lev = action
        .get("newLeverage")
        .and_then(Value::as_str)
        .and_then(parse_decimal_scaled);

    let increase = match (find_perp(ctx, &market), new_lev) {
        // Open position (size_base != 0) AND requested leverage strictly higher.
        (Some(p), Some(nl)) if !p.size_base.is_zero() => {
            parse_decimal_scaled(p.leverage.value.as_str()).is_some_and(|cur| nl > cur)
        }
        _ => false,
    };

    Ok(json!({ "leverageIncrease": increase }))
}

/// `perp.stop_trigger_misplaced` — PERP-12. Is the stop/take-profit `triggerPrice`
/// on the wrong side of mark for its side + orderKind?
///
/// readKind: `derived` (no durable-state read)
///
/// outputs: `misplaced`: Bool — `$.result.misplaced`
///
/// Correct placement (stop closes a position at a loss, take-profit at a gain):
///   - long  `stop_market/stop_limit`      → trigger BELOW mark (misplaced if ≥)
///   - long  `take_profit`*                → trigger ABOVE mark (misplaced if ≤)
///   - short `stop_market/stop_limit`      → trigger ABOVE mark (misplaced if ≤)
///   - short `take_profit`*                → trigger BELOW mark (misplaced if ≥)
///
/// Unknown side / orderKind / unparsable prices → `false` (no false-positive).
fn perp_stop_trigger_misplaced(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let side = action_side(action);
    let kind = action.get("orderKind").and_then(Value::as_str);
    let trigger = action
        .get("triggerPrice")
        .and_then(Value::as_str)
        .and_then(parse_decimal_scaled);
    let mark = action
        .get("markPrice")
        .and_then(Value::as_str)
        .and_then(parse_decimal_scaled);

    let is_take_profit = matches!(kind, Some("take_profit" | "take_profit_limit"));
    let is_stop = matches!(kind, Some("stop_market" | "stop_limit"));

    let misplaced = match (side, trigger, mark) {
        (Some("long"), Some(t), Some(m)) if is_stop => t >= m,
        (Some("long"), Some(t), Some(m)) if is_take_profit => t <= m,
        (Some("short"), Some(t), Some(m)) if is_stop => t <= m,
        (Some("short"), Some(t), Some(m)) if is_take_profit => t >= m,
        _ => false,
    };

    Ok(json!({ "misplaced": misplaced }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::live_field::DataSource;
    use policy_state::position::Position;
    use policy_state::primitives::{
        Address, ChainId, Decimal, MarketRef, Price, ProtocolRef, SignedI256, Time, VenueRef,
    };
    use policy_state::{LiveField, WalletId, WalletState};

    fn chain() -> ChainId {
        ChainId::arbitrum()
    }

    fn wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        )
    }

    fn src() -> DataSource {
        DataSource::UserSupplied
    }

    fn live<T>(v: T) -> LiveField<T> {
        LiveField::new(v, src(), Time::from_unix(1_700_000_000))
    }

    fn market_ref(symbol: &str) -> MarketRef {
        MarketRef {
            symbol: symbol.to_owned(),
            venue: VenueRef {
                name: "hyperliquid".into(),
                chain: Some(chain()),
            },
        }
    }

    /// A perp position on `market` with the given side, raw `size_base`,
    /// `notional_usd`, leverage, margin mode, unrealized pnl, and optional liq.
    #[allow(clippy::too_many_arguments)]
    fn perp_position(
        id: &str,
        market: &str,
        side: PerpSide,
        size_base: u64,
        notional_usd: u64,
        leverage: &str,
        margin_mode: MarginMode,
        unrealized_pnl: i64,
        liq_price: Option<&str>,
    ) -> Position {
        let p = PerpPosition {
            venue: VenueRef {
                name: "hyperliquid".into(),
                chain: Some(chain()),
            },
            market: market_ref(market),
            side,
            size_base: U256::from(size_base),
            notional_usd: U256::from(notional_usd),
            collateral: vec![],
            entry_price: Price::new("3000"),
            margin_mode,
            mark_price: live(Price::new("3050")),
            liq_price: live(liq_price.map(Price::new)),
            unrealized_pnl: live(SignedI256::try_from(unrealized_pnl).unwrap()),
            funding_owed: live(SignedI256::try_from(0i64).unwrap()),
            leverage: live(Decimal::new(leverage)),
        };
        Position {
            id: id.to_owned(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: Some(chain()),
            kind: PositionKind::PerpPosition(p),
            primitives_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: src(),
        }
    }

    fn empty_state() -> WalletState {
        WalletState::new(wallet_id())
    }

    #[test]
    fn order_leverage_explicit_field() {
        let params = json!({ "action": { "leverage": "5", "size": { "kind": "base_amount", "amount": "0x1" } } });
        let out = dispatch(
            "perp.order_leverage",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["leverage"], json!("5.0000"));
    }

    #[test]
    fn order_leverage_implied_arm() {
        let params = json!({ "action": { "size": { "kind": "leverage_implied", "collateral": "0x1", "leverage": "12.5" } } });
        let out = dispatch(
            "perp.order_leverage",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["leverage"], json!("12.5000"));
    }

    #[test]
    fn funding_adverse_long_positive_is_adverse() {
        let params = json!({ "action": { "side": "long", "fundingRate": "0.0003" } });
        let out = dispatch(
            "perp.funding_adverse_rate",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["adverseRate"], json!("0.0003"));
    }

    #[test]
    fn funding_adverse_long_negative_is_favourable() {
        let params = json!({ "action": { "side": "long", "fundingRate": "-0.0003" } });
        let out = dispatch(
            "perp.funding_adverse_rate",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["adverseRate"], json!("0.0000"));
    }

    #[test]
    fn funding_adverse_short_negative_reports_magnitude() {
        let params = json!({ "action": { "side": "short", "fundingRate": "-0.0005" } });
        let out = dispatch(
            "perp.funding_adverse_rate",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["adverseRate"], json!("0.0005"));
    }

    #[test]
    fn stop_trigger_long_stop_below_mark_ok() {
        // Long stop-loss correctly placed below mark → not misplaced.
        let params = json!({ "action": { "side": "long", "orderKind": "stop_market", "triggerPrice": "2900", "markPrice": "3050" } });
        let out = dispatch(
            "perp.stop_trigger_misplaced",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["misplaced"], json!(false));
    }

    #[test]
    fn stop_trigger_long_stop_above_mark_misplaced() {
        let params = json!({ "action": { "side": "long", "orderKind": "stop_market", "triggerPrice": "3200", "markPrice": "3050" } });
        let out = dispatch(
            "perp.stop_trigger_misplaced",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["misplaced"], json!(true));
    }

    #[test]
    fn stop_trigger_long_take_profit_below_mark_misplaced() {
        let params = json!({ "action": { "side": "long", "orderKind": "take_profit", "triggerPrice": "2900", "markPrice": "3050" } });
        let out = dispatch(
            "perp.stop_trigger_misplaced",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["misplaced"], json!(true));
    }

    #[test]
    fn stop_trigger_short_stop_above_mark_ok() {
        let params = json!({ "action": { "side": "short", "orderKind": "stop_limit", "triggerPrice": "3200", "markPrice": "3050" } });
        let out = dispatch(
            "perp.stop_trigger_misplaced",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["misplaced"], json!(false));
    }

    #[test]
    fn margin_mode_isolated_to_cross_flags() {
        let mut state = empty_state();
        state.positions.push(perp_position(
            "ETH-USD",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Isolated,
            0,
            Some("2500"),
        ));
        let params = json!({ "market": "ETH-USD", "action": { "market": { "symbol": "ETH-USD" }, "newMode": "cross" } });
        let out = dispatch(
            "perp.margin_mode_transition",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["isolatedToCross"], json!(true));
    }

    #[test]
    fn margin_mode_already_cross_not_flagged() {
        let mut state = empty_state();
        state.positions.push(perp_position(
            "ETH-USD",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Cross,
            0,
            Some("2500"),
        ));
        let params = json!({ "market": "ETH-USD", "action": { "market": { "symbol": "ETH-USD" }, "newMode": "cross" } });
        let out = dispatch(
            "perp.margin_mode_transition",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["isolatedToCross"], json!(false));
    }

    #[test]
    fn position_state_reads_side_leverage_and_pnl_pct() {
        let mut state = empty_state();
        // +150 pnl on 3000 notional → 5.00%.
        state.positions.push(perp_position(
            "ETH-USD",
            "ETH-USD",
            PerpSide::Short,
            1_000,
            3_000,
            "7.5",
            MarginMode::Cross,
            150,
            Some("3500"),
        ));
        let params = json!({ "chain_id": 42161, "owner": "0x000000000000000000000000000000000000a01c", "market": "ETH-USD" });
        let out = dispatch("perp.position_state", &params, &FactCtx { state: &state }).unwrap();
        assert_eq!(out["side"], json!("short"));
        assert_eq!(out["leverage"], json!("7.5000"));
        assert_eq!(out["unrealizedPnlPct"], json!("5.0000"));
    }

    #[test]
    fn position_state_negative_pnl_pct() {
        let mut state = empty_state();
        // -60 pnl on 3000 notional → -2.00%.
        state.positions.push(perp_position(
            "ETH-USD",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Cross,
            -60,
            None,
        ));
        let params = json!({ "market": "ETH-USD" });
        let out = dispatch("perp.position_state", &params, &FactCtx { state: &state }).unwrap();
        assert_eq!(out["unrealizedPnlPct"], json!("-2.0000"));
    }

    #[test]
    fn position_state_absent_market_is_neutral() {
        let params = json!({ "market": "DOGE-USD" });
        let out = dispatch(
            "perp.position_state",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["side"], json!(""));
        assert_eq!(out["leverage"], json!("0.0000"));
        assert_eq!(out["unrealizedPnlPct"], json!("0.0000"));
    }

    #[test]
    fn crosses_zero_decrease_overshoots() {
        let mut state = empty_state();
        state.positions.push(perp_position(
            "pos-1",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Cross,
            0,
            Some("2500"),
        ));
        // Decrease 1500 base on a 1000-base long → flips through zero.
        let params = json!({ "action": { "positionId": "pos-1", "reduceOnly": true, "size": { "kind": "base_amount", "amount": format!("{:#x}", U256::from(1_500u64)) } } });
        let out = dispatch("perp.crosses_zero", &params, &FactCtx { state: &state }).unwrap();
        assert_eq!(out["crossesZero"], json!(true));
    }

    #[test]
    fn crosses_zero_partial_decrease_does_not_flip() {
        let mut state = empty_state();
        state.positions.push(perp_position(
            "pos-1",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Cross,
            0,
            Some("2500"),
        ));
        let params = json!({ "action": { "positionId": "pos-1", "reduceOnly": true, "size": { "kind": "base_amount", "amount": format!("{:#x}", U256::from(400u64)) } } });
        let out = dispatch("perp.crosses_zero", &params, &FactCtx { state: &state }).unwrap();
        assert_eq!(out["crossesZero"], json!(false));
    }

    #[test]
    fn leverage_increase_true_when_higher() {
        let mut state = empty_state();
        state.positions.push(perp_position(
            "ETH-USD",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Cross,
            0,
            Some("2500"),
        ));
        let params = json!({ "market": "ETH-USD", "action": { "market": { "symbol": "ETH-USD" }, "newLeverage": "10" } });
        let out = dispatch(
            "perp.leverage_increase",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["leverageIncrease"], json!(true));
    }

    #[test]
    fn leverage_increase_false_when_lower_or_equal() {
        let mut state = empty_state();
        state.positions.push(perp_position(
            "ETH-USD",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "10",
            MarginMode::Cross,
            0,
            Some("2500"),
        ));
        let params = json!({ "market": "ETH-USD", "action": { "newLeverage": "5" } });
        let out = dispatch(
            "perp.leverage_increase",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["leverageIncrease"], json!(false));
    }

    #[test]
    fn leverage_increase_false_when_no_position() {
        let params = json!({ "market": "ETH-USD", "action": { "newLeverage": "10" } });
        let out = dispatch(
            "perp.leverage_increase",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["leverageIncrease"], json!(false));
    }

    #[test]
    fn notional_to_collateral_quote_amount_exact() {
        // 2000 USD notional / 10000 USD collateral = 0.2000.
        let params = json!({
            "action": {
                "size": { "kind": "quote_amount", "amountUsd": format!("{:#x}", U256::from(2_000u64)) },
                "userAccountState": { "totalCollateralUsd": format!("{:#x}", U256::from(10_000u64)) }
            }
        });
        let out = dispatch(
            "perp.notional_to_collateral",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["ratio"], json!("0.2000"));
    }

    #[test]
    fn cross_total_exposure_folds_cross_positions() {
        let mut state = empty_state();
        // Two cross positions (3000 + 5000) + one isolated (ignored).
        state.positions.push(perp_position(
            "ETH-USD",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Cross,
            0,
            Some("2500"),
        ));
        state.positions.push(perp_position(
            "BTC-USD",
            "BTC-USD",
            PerpSide::Short,
            100,
            5_000,
            "3",
            MarginMode::Cross,
            0,
            Some("70000"),
        ));
        state.positions.push(perp_position(
            "SOL-USD",
            "SOL-USD",
            PerpSide::Long,
            10,
            9_000,
            "2",
            MarginMode::Isolated,
            0,
            None,
        ));
        // New quote order 2000 USD. Total = 3000+5000+2000 = 10000 / 20000 = 0.5000.
        let params = json!({
            "action": {
                "size": { "kind": "quote_amount", "amountUsd": format!("{:#x}", U256::from(2_000u64)) },
                "userAccountState": { "totalCollateralUsd": format!("{:#x}", U256::from(20_000u64)) }
            }
        });
        let out = dispatch(
            "perp.cross_total_exposure_ratio",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["ratio"], json!("0.5000"));
    }

    #[test]
    fn liq_distance_bp_from_existing_position() {
        let mut state = empty_state();
        // liq 2500 vs mark 3050 → |3050-2500|/3050 = 1803 bp.
        state.positions.push(perp_position(
            "pos-1",
            "ETH-USD",
            PerpSide::Long,
            1_000,
            3_000,
            "5",
            MarginMode::Cross,
            0,
            Some("2500"),
        ));
        let params = json!({ "action": { "positionId": "pos-1", "markPrice": "3050" } });
        let out = dispatch("perp.liq_distance_bp", &params, &FactCtx { state: &state }).unwrap();
        assert_eq!(out["liqDistanceBp"], json!(1803));
    }

    #[test]
    fn liq_distance_bp_no_position_is_safe_sentinel() {
        let params =
            json!({ "action": { "market": { "symbol": "ETH-USD" }, "markPrice": "3050" } });
        let out = dispatch(
            "perp.liq_distance_bp",
            &params,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap();
        assert_eq!(out["liqDistanceBp"], json!(10_000));
    }

    #[test]
    fn unknown_method_errors() {
        let err = dispatch(
            "perp.not_a_real_method",
            &Value::Null,
            &FactCtx {
                state: &empty_state(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }
}
