//! `launchpad.*` enrichment-fact stubs — launchpad sale / allocation facts served
//! by the **sim-server** fact host (NOT the external policy-rpc daemon).
//! Generated from `schema/method-catalog.json` `planned` entries whose `name`
//! starts with `launchpad.` AND whose `server == "sim-server"`.
//!
//! The two `launchpad.*` methods with `server == "external"`
//! (`launchpad.curve_premium_bp`, `launchpad.pay_token_match`) are intentionally
//! ABSENT here — they deploy to the external layer, not this fact host.
//!
//! Each fact below is a not-implemented stub: the dispatch arm exists (so the
//! method is routable and the server boots), but the body returns
//! [`FactError::NotImplemented`] until a dev fills it in. The inner `dispatch`
//! match is COMPLETE and FROZEN at scaffold time — do not edit it when wiring
//! bodies; add logic inside the per-method fns only.
//!
//! ## Param shape contract
//!
//! Like the rest of `facts/`, `params` arrive as **lowered Cedar** shapes from
//! the extension (not `simulation-state` shapes):
//!   - `chain_id`: string (e.g. `"eip155:1"`)
//!   - `owner`: hex address string
//!   - `action`: the lowered launchpad `Commit`/claim action (carries the target
//!     sale contract / commit amount the fact compares against state).

use serde_json::{json, Value};

use policy_state::position::PositionKind;
use policy_state::primitives::{ChainId, Decimal, U256};
use policy_state::token::holding::TokenHolding;
use policy_state::token::TokenKey;

use super::params::{param_action, param_chain_id};
use super::FactCtx;
use super::FactError;

// ---------------------------------------------------------------------------
// USD-valuation helpers (inlined; the equivalents in `valuation.rs` are private
// to that module and the concurrency rules forbid editing shared files). Same
// `U256` integer-math idiom: no float, no `Decimal` arithmetic.
// ---------------------------------------------------------------------------

/// Fractional digits the USD `usd` result is rendered to. Matches the
/// `over_balance_4dp` / `valuation.*` convention so Cedar `.greaterThan(...)`
/// thresholds compare against a consistent 4-dp fixed-point shape.
const USD_DP: u32 = 4;

/// USD-decimal sentinel for "unpriced" — when a paid leg / the in-flight commit
/// token has no `price_usd` `LiveField` we cannot value it, so a conservative
/// huge number is emitted (a cumulative-cap-deny policy trips rather than
/// silently passing an unvalued commit). Mirrors `OVER_BALANCE_SENTINEL`.
const UNPRICED_USD_SENTINEL: &str = "1000000000.0000";

/// Parse a decimal string (`"3500.25"`, `"0"`, `"1"`) into `(mantissa,
/// frac_digits)` such that `value == mantissa / 10^frac_digits`. Returns `None`
/// on any non-plain-numeric input (negative / scientific / empty) so callers
/// fall back to the unpriced sentinel rather than fabricate a price.
fn parse_decimal_scaled(d: &Decimal) -> Option<(U256, u32)> {
    let s = d.as_str().trim();
    let s = s.strip_prefix('+').unwrap_or(s);
    if s.starts_with('-') || s.is_empty() {
        return None;
    }
    let (int_part, frac_part) = s.split_once('.').map_or((s, ""), |(i, f)| (i, f));
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{int_part}{frac_part}");
    let digits = if digits.is_empty() { "0" } else { &digits };
    let mantissa = U256::from_str_radix(digits, 10).ok()?;
    let frac_digits = u32::try_from(frac_part.len()).ok()?;
    Some((mantissa, frac_digits))
}

/// USD value of `raw_amount` raw token units priced at `price` (USD per whole
/// token), as an integer `value * 10^USD_DP` via `U256` integer math:
/// `raw * price_mantissa * 10^USD_DP / (10^decimals * 10^price_frac)`.
/// Saturating multiplication keeps adversarial `U256::MAX` inputs in the
/// conservative (huge) direction. `None` when `price` is unparseable.
fn usd_value_scaled(raw_amount: U256, decimals: u8, price: &Decimal) -> Option<U256> {
    let (price_mantissa, price_frac) = parse_decimal_scaled(price)?;
    let numerator = raw_amount
        .saturating_mul(price_mantissa)
        .saturating_mul(U256::from(10u64).pow(U256::from(USD_DP)));
    let denom_pow = u64::from(decimals) + u64::from(price_frac);
    let denominator = U256::from(10u64).pow(U256::from(denom_pow));
    Some(numerator / denominator)
}

/// Render an integer already representing `value * 10^USD_DP` as a `whole.frac`
/// decimal string with exactly [`USD_DP`] fractional digits.
fn render_scaled(scaled: U256) -> String {
    let scale = U256::from(10u64).pow(U256::from(USD_DP));
    let whole = scaled / scale;
    let frac = scaled % scale;
    let dp = USD_DP as usize;
    format!("{whole}.{frac:0dp$}")
}

/// Read a holding's `price_usd` `LiveField` value (a [`Decimal`]); `None` when
/// the holding is unpriced.
fn holding_price(holding: &TokenHolding) -> Option<&Decimal> {
    holding.price_usd.as_ref().map(|lf| &lf.value)
}

/// Route a `launchpad.*` method to its fact implementation.
///
/// FROZEN: one arm per sim-server method in this namespace plus the catch-all.
/// Devs filling in bodies must never edit this match.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] for an unregistered method, or whatever
/// error the per-method fn surfaces (currently [`FactError::NotImplemented`]
/// until the body is filled in).
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "launchpad.sale_match" => sale_match(params, ctx),
        "launchpad.cumulative_committed_usd" => cumulative_committed_usd(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// LP-01 `launchpad.sale_match` — whether the claim/commit target matches the
/// original sale recorded in wallet state (`LaunchpadAllocation.sale_id`).
///
/// readKind: `direct`
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `owner`: String (required) — `$.root.from`
/// - `action`: Action (required) — `$.action`; carries the claim/commit target
///   sale id (`saleId`) compared against the recorded `sale_id`.
///
/// Catalog outputs:
/// - `targetMatchesSale`: Bool — from `$.result.targetMatchesSale`
///
/// The lowered launchpad action carries the target sale as the `saleId` string
/// field (see `policy-engine` `lowering_v2::launchpad::{commit,claim_allocation,
/// refund,withdraw_commit}`, all of which emit `m.insert("saleId", ...)`). We
/// scan `WalletState.positions` for any `LaunchpadAllocation` whose `sale_id`
/// equals it; a match means the action targets a sale the wallet actually holds
/// an allocation in (`positions(launchpad_allocation).sale_id`). No
/// `tokens`/price read needed.
fn sale_match(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;
    let target_sale = action
        .get("saleId")
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams("action missing `saleId`".to_owned()))?;

    let matches = ctx.state.positions.iter().any(|pos| match &pos.kind {
        PositionKind::LaunchpadAllocation(alloc) => alloc.sale_id == target_sale,
        _ => false,
    });

    Ok(json!({ "targetMatchesSale": matches }))
}

/// LP-03 `launchpad.cumulative_committed_usd` — USD sum of every currently-held
/// `LaunchpadAllocation.paid` leg across all open sales PLUS the in-flight Commit
/// amount, valued at refrigerated DB price. Folds wallet state to one decimal
/// because Cedar cannot fold a `Vec<(TokenRef, U256)>` across positions nor
/// multiply by price.
///
/// readKind: `fold`
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `owner`: String (required) — `$.root.from`
/// - `action`: Action (required) — `$.action`; the in-flight Commit, whose
///   `amount` (U256 hex) priced via `payToken` is added to the allocation sum.
///
/// Catalog outputs:
/// - `usd`: decimal — from `$.result.usd`
///
/// Valuation: each leg is `raw_amount * price / 10^decimals` (price = the paid
/// token's `TokenHolding.price_usd.value`, decimals = `TokenHolding.decimals`),
/// summed in the `10^USD_DP`-scaled integer domain. A leg whose token is absent
/// from `tokens` or unpriced cannot be valued from real fields; rather than
/// fabricate a price we emit [`UNPRICED_USD_SENTINEL`] so a cumulative-cap-deny
/// policy trips conservatively. The fold spans ALL `LaunchpadAllocation`s
/// regardless of `Position.chain` (positions may be off-chain; the catalog's
/// `stateDependency` scopes by allocation, not chain).
fn cumulative_committed_usd(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let action = param_action(params, "action")?;

    let mut total_scaled = U256::ZERO;

    // Existing allocations: sum each `paid` leg, valued at the leg token's price.
    for pos in &ctx.state.positions {
        let PositionKind::LaunchpadAllocation(alloc) = &pos.kind else {
            continue;
        };
        for (token_ref, raw_amount) in &alloc.paid {
            let Some(leg_scaled) = value_leg(ctx, &token_ref.key, *raw_amount) else {
                return Ok(json!({ "usd": UNPRICED_USD_SENTINEL }));
            };
            total_scaled = total_scaled.saturating_add(leg_scaled);
        }
    }

    // In-flight Commit: add `action.amount` priced via `action.payToken`. A
    // claim/refund action carries no `amount`/`payToken`; those add nothing
    // here (the cumulative sum is then just the existing allocations).
    if let (Some(amount), Some(pay_key)) = (commit_amount(action), commit_pay_key(action, &chain)) {
        let Some(leg_scaled) = value_leg(ctx, &pay_key, amount) else {
            return Ok(json!({ "usd": UNPRICED_USD_SENTINEL }));
        };
        total_scaled = total_scaled.saturating_add(leg_scaled);
    }

    Ok(json!({ "usd": render_scaled(total_scaled) }))
}

/// USD value of `raw_amount` of the token identified by `key`, in the
/// `10^USD_DP`-scaled integer domain. `None` when the token is not held (no
/// `decimals` to scale by) or has no `price_usd` — the caller then emits the
/// unpriced sentinel rather than fabricate a value.
fn value_leg(ctx: &FactCtx, key: &TokenKey, raw_amount: U256) -> Option<U256> {
    let holding = ctx.state.tokens.get(key)?;
    let price = holding_price(holding)?;
    usd_value_scaled(raw_amount, holding.decimals, price)
}

/// The in-flight Commit's `amount` (U256 hex) from the lowered action, or `None`
/// for a non-Commit launchpad action (claim/refund carry no `amount`).
fn commit_amount(action: &Value) -> Option<U256> {
    let s = action.get("amount").and_then(Value::as_str)?;
    U256::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// Reconstruct the [`TokenKey`] of the in-flight Commit's `payToken` (a lowered
/// `Core::TokenRef`: `{ "key": { "standard": "erc20", "chain", "address" } }`).
/// Only ERC20 pay tokens are valued (the launchpad `Commit` pay token is ERC20
/// or native; native carries no holding price here). `None` for a non-Commit
/// action or a non-ERC20 / malformed pay token — the leg is then skipped.
fn commit_pay_key(action: &Value, chain: &ChainId) -> Option<TokenKey> {
    let key = action.get("payToken").and_then(|t| t.get("key"))?;
    if key.get("standard").and_then(Value::as_str) != Some("erc20") {
        return None;
    }
    let addr = key.get("address").and_then(Value::as_str)?;
    let address = addr.parse().ok()?;
    Some(TokenKey::Erc20 {
        chain: chain.clone(),
        address,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::position::{
        LaunchpadAllocation, Position, PositionKind, VestCurve, VestSchedule,
    };
    use policy_state::primitives::{Address, ChainId, Price, ProtocolRef, Time, U256};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{BaseCategory, TokenKind};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::{WalletId, WalletState};

    const PAY_TOKEN: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn pay_addr() -> Address {
        Address::from_str(PAY_TOKEN).unwrap()
    }

    fn pay_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: pay_addr(),
        }
    }

    fn wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        )
    }

    fn src() -> DataSource {
        DataSource::OnchainView {
            chain: chain(),
            contract: pay_addr(),
            function: "balanceOf(address)".into(),
            decoder_id: "erc20_balance".into(),
        }
    }

    /// A priced ERC20 holding for the pay token (6 decimals, `price` USD each).
    fn priced_holding(price: &str) -> TokenHolding {
        TokenHolding {
            key: pay_key(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: None,
            },
            symbol: "USDC".to_owned(),
            decimals: 6,
            balance: Balance::fungible(U256::from(0u64)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(LiveField::new(
                Price::new(price),
                src(),
                Time::from_unix(1_700_000_000),
            )),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: src(),
        }
    }

    /// An unpriced ERC20 holding for the pay token.
    fn unpriced_holding() -> TokenHolding {
        let mut h = priced_holding("1");
        h.price_usd = None;
        h
    }

    fn vest() -> VestSchedule {
        VestSchedule {
            start: Time::from_unix(1_739_000_000),
            cliff: None,
            end: None,
            curve: VestCurve::Linear,
            total: U256::from(1_000u64),
        }
    }

    /// A `LaunchpadAllocation` position for `sale_id` paying `paid_raw` raw units
    /// of the pay token.
    fn alloc_position(sale_id: &str, paid_raw: u64) -> Position {
        Position {
            id: format!("lp-{sale_id}"),
            protocol: ProtocolRef::new("coinlist"),
            chain: Some(chain()),
            kind: PositionKind::LaunchpadAllocation(LaunchpadAllocation {
                platform: ProtocolRef::new("coinlist"),
                sale_id: sale_id.to_owned(),
                paid: vec![(TokenRef::new(pay_key()), U256::from(paid_raw))],
                allocated: (TokenRef::new(pay_key()), U256::from(0u64)),
                vest: vest(),
                claimed: U256::ZERO,
                claimable_now: U256::ZERO,
            }),
            primitives_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: src(),
        }
    }

    fn state_with(positions: Vec<Position>, holding: Option<TokenHolding>) -> WalletState {
        let mut state = WalletState::new(wallet_id());
        state.positions = positions;
        if let Some(h) = holding {
            state.tokens.insert(pay_key(), h);
        }
        state
    }

    /// Lowered `payToken` (`Core::TokenRef`) for the ERC20 pay token.
    fn pay_token_param() -> Value {
        json!({
            "key": {
                "standard": "erc20",
                "chain": chain().to_string(),
                "address": PAY_TOKEN
            }
        })
    }

    /// A lowered launchpad `Commit` action with `saleId` + `amount` + `payToken`.
    fn commit_action(sale_id: &str, amount: U256) -> Value {
        json!({
            "saleId": sale_id,
            "amount": format!("{amount:#x}"),
            "payToken": pay_token_param(),
        })
    }

    /// A lowered claim/refund-style action carrying only `saleId` (no amount).
    fn claim_action(sale_id: &str) -> Value {
        json!({ "saleId": sale_id })
    }

    fn params(action: &Value) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "action": action,
        })
    }

    // --- sale_match ---------------------------------------------------------

    #[test]
    fn sale_match_true_when_alloc_sale_id_matches() {
        let state = state_with(vec![alloc_position("sale-42", 1_000)], None);
        let out = dispatch(
            "launchpad.sale_match",
            &params(&claim_action("sale-42")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["targetMatchesSale"], json!(true));
    }

    #[test]
    fn sale_match_false_when_no_alloc_matches() {
        let state = state_with(vec![alloc_position("sale-1", 1_000)], None);
        let out = dispatch(
            "launchpad.sale_match",
            &params(&claim_action("sale-999")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["targetMatchesSale"], json!(false));
    }

    #[test]
    fn sale_match_false_with_no_launchpad_positions() {
        let state = state_with(vec![], None);
        let out = dispatch(
            "launchpad.sale_match",
            &params(&claim_action("sale-42")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["targetMatchesSale"], json!(false));
    }

    #[test]
    fn sale_match_bad_params_without_sale_id() {
        let state = state_with(vec![], None);
        let err = dispatch(
            "launchpad.sale_match",
            &params(&json!({})),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    // --- cumulative_committed_usd ------------------------------------------

    #[test]
    fn cumulative_sums_paid_legs_plus_in_flight_commit() {
        // One existing allocation paid 1_000_000 raw (= 1.0 USDC @ 6dp) at $1,
        // plus an in-flight commit of 2_000_000 raw (= 2.0 USDC) at $1 → $3.0000.
        let state = state_with(
            vec![alloc_position("sale-1", 1_000_000)],
            Some(priced_holding("1")),
        );
        let out = dispatch(
            "launchpad.cumulative_committed_usd",
            &params(&commit_action("sale-1", U256::from(2_000_000u64))),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["usd"], json!("3.0000"));
    }

    #[test]
    fn cumulative_prices_at_nonunit_price() {
        // 1_000_000 raw (1.0 token @6dp) at $2.50, no in-flight commit → $2.5000.
        let state = state_with(
            vec![alloc_position("sale-1", 1_000_000)],
            Some(priced_holding("2.5")),
        );
        let out = dispatch(
            "launchpad.cumulative_committed_usd",
            &params(&claim_action("sale-1")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["usd"], json!("2.5000"));
    }

    #[test]
    fn cumulative_zero_with_no_allocations_and_claim_action() {
        let state = state_with(vec![], None);
        let out = dispatch(
            "launchpad.cumulative_committed_usd",
            &params(&claim_action("sale-1")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["usd"], json!("0.0000"));
    }

    #[test]
    fn cumulative_unpriced_paid_leg_yields_sentinel() {
        // Paid leg token is held but unpriced → sentinel (conservative).
        let state = state_with(
            vec![alloc_position("sale-1", 1_000_000)],
            Some(unpriced_holding()),
        );
        let out = dispatch(
            "launchpad.cumulative_committed_usd",
            &params(&claim_action("sale-1")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["usd"], json!(UNPRICED_USD_SENTINEL));
    }

    #[test]
    fn cumulative_unpriced_in_flight_commit_yields_sentinel() {
        // No existing allocations, but the in-flight commit token is absent from
        // `tokens` (no decimals/price) → sentinel.
        let state = state_with(vec![], None);
        let out = dispatch(
            "launchpad.cumulative_committed_usd",
            &params(&commit_action("sale-1", U256::from(2_000_000u64))),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["usd"], json!(UNPRICED_USD_SENTINEL));
    }

    // --- dispatch wiring ----------------------------------------------------

    #[test]
    fn unknown_method_errors() {
        let state = state_with(vec![], None);
        let err = dispatch(
            "launchpad.not_a_method",
            &json!({}),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }
}
