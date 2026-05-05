//! Action → `PolicyRequest` lowering, plus the Stage-3 oracle injection
//! step. These are the helpers `Adapter::into_request`'s default impl uses
//! to turn a built `Action` into the JSON shape the Cedar evaluator consumes.
//!
//! Layout:
//! - `enrich_with_usd` — Stage 3 (oracle prices → `AmountSpec.usd`).
//! - `request_from_action` — Stage 4 prep (Action → `PolicyRequest`
//!   principal/action/resource/entities/context).
//! - decimal-string arithmetic helpers used to compute USD valuations
//!   without f64 drift.

use crate::core::{Action, AmountSpec, SwapAction, UsdValuation};
use crate::oracle::Oracle;
use crate::policy::PolicyRequest;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Stage 3: oracle injection.
// ---------------------------------------------------------------------------

/// Walk a swap action's amount specs and populate `usd` valuations from the
/// oracle. Missing prices leave `usd` as `None` — fail-open by default; the
/// policy layer chooses fail-closed via `has "usd"`.
pub fn enrich_with_usd(action: &mut Action, oracle: &dyn Oracle) {
    match action {
        Action::Swap(s) => populate_usd(s, oracle),
        Action::Multi(m) => enrich_actions_with_usd(&mut m.children, oracle),
        Action::Other { .. } => {}
    }
}

pub fn enrich_actions_with_usd(actions: &mut [Action], oracle: &dyn Oracle) {
    for action in actions {
        enrich_with_usd(action, oracle);
    }
}

fn populate_usd(s: &mut SwapAction, oracle: &dyn Oracle) {
    if let Ok(v) = oracle.price(&s.input_amount.token) {
        s.input_amount.usd = Some(scaled_usd(
            &s.input_amount.raw,
            s.input_amount.token.decimals,
            &v,
        ));
    }
    if let Some(min) = s.min_output_amount.as_mut() {
        if let Ok(v) = oracle.price(&min.token) {
            min.usd = Some(scaled_usd(&min.raw, min.token.decimals, &v));
        }
    }
}

fn scaled_usd(raw: &str, decimals: u32, valuation: &UsdValuation) -> UsdValuation {
    let value = multiply_decimal_strings(raw, decimals, &valuation.value);
    UsdValuation {
        value,
        as_of_ts: valuation.as_of_ts,
        sources: valuation.sources.clone(),
        stale_sec: valuation.stale_sec,
    }
}

/// Compute `(raw_int / 10^decimals) * price`, returning a decimal string with
/// up to 4 fractional places (matching Cedar's Decimal precision).
pub(crate) fn multiply_decimal_strings(raw: &str, decimals: u32, price: &str) -> String {
    use alloy_primitives::U256;

    let raw_u = U256::from_str_radix(raw, 10).unwrap_or(U256::ZERO);

    const PRICE_SCALE: u32 = 4;
    let price_int = decimal_to_fixed(price, PRICE_SCALE);

    let product = raw_u.saturating_mul(U256::from(price_int));
    let scale = U256::from(10u64).pow(U256::from(decimals));
    let scaled = if scale.is_zero() {
        product
    } else {
        product / scale
    };

    fixed_to_decimal(scaled, PRICE_SCALE)
}

fn decimal_to_fixed(s: &str, scale: u32) -> u128 {
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    let mut frac_padded = String::from(frac);
    while frac_padded.len() < scale as usize {
        frac_padded.push('0');
    }
    if frac_padded.len() > scale as usize {
        frac_padded.truncate(scale as usize);
    }
    let combined = format!("{whole}{frac_padded}");
    combined.parse::<u128>().unwrap_or(0)
}

fn fixed_to_decimal(value: alloy_primitives::U256, scale: u32) -> String {
    let value_str = value.to_string();
    let scale = scale as usize;
    let padded = if value_str.len() <= scale {
        format!("{}{}", "0".repeat(scale + 1 - value_str.len()), value_str)
    } else {
        value_str
    };
    let split = padded.len() - scale;
    let (whole, frac) = padded.split_at(split);
    if scale == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac}")
    }
}

// ---------------------------------------------------------------------------
// Stage 4 prep: Action → PolicyRequest.
// ---------------------------------------------------------------------------

/// Build a `PolicyRequest` from a fully-enriched `Action`. This is the public
/// "Action → Cedar request" conversion; `Adapter::into_request` calls it.
pub fn request_from_action(action: &Action) -> PolicyRequest {
    let principal = format!(r#"Wallet::"{}""#, action.actor().as_str());
    let action_uid = format!(r#"Action::"{}""#, action.kind());
    let resource = match action {
        Action::Swap(s) => format!(r#"Protocol::"{}""#, s.protocol_id),
        Action::Multi(_) => String::from(r#"Protocol::"multi""#),
        Action::Other { .. } => String::from(r#"Protocol::"unknown""#),
    };
    let entities = action_entities(action);
    let context = action_context(action);
    PolicyRequest::new(principal, action_uid, resource, entities, context)
}

/// Build one or more leaf `PolicyRequest`s from an action tree. `Multi`
/// actions are structural: their children are evaluated individually so
/// existing leaf policies such as `action == Action::"swap"` keep working
/// without policy edits.
pub fn requests_from_action(action: &Action) -> Vec<PolicyRequest> {
    match action {
        Action::Multi(m) => requests_from_actions(&m.children),
        Action::Swap(_) | Action::Other { .. } => vec![request_from_action(action)],
    }
}

pub fn requests_from_actions(actions: &[Action]) -> Vec<PolicyRequest> {
    actions.iter().flat_map(requests_from_action).collect()
}

fn action_entities(action: &Action) -> Value {
    let resource_id = match action {
        Action::Swap(s) => s.protocol_id.clone(),
        Action::Multi(_) => "multi".into(),
        Action::Other { .. } => "unknown".into(),
    };
    let actor_id = action.actor().as_str();
    json!([
        { "uid": { "type": "Wallet",   "id": actor_id },     "attrs": {}, "parents": [] },
        { "uid": { "type": "Protocol", "id": resource_id },   "attrs": {}, "parents": [] },
    ])
}

fn action_context(action: &Action) -> Value {
    // Optional fields are *omitted* (not set to null) — Cedar has no null
    // type, and policies guard with `context has "field"`.
    match action {
        Action::Swap(s) => {
            let mut m = serde_json::Map::new();
            m.insert("inputAmount".into(), amount_json(&s.input_amount));
            if let Some(min) = &s.min_output_amount {
                m.insert("minOutputAmount".into(), amount_json(min));
            }
            if let Some(fee) = s.fee_bips {
                m.insert("feeBips".into(), Value::from(fee as i64));
            }
            m.insert("target".into(), Value::from(s.target.0.clone()));
            m.insert("recipient".into(), Value::from(s.recipient.0.clone()));
            m.insert("protocolId".into(), Value::from(s.protocol_id.clone()));
            Value::Object(m)
        }
        Action::Multi(m) => json!({
            "target": m.target.0,
            "childCount": m.children.len() as i64,
            "childKinds": m.children.iter().map(|a| Value::from(a.kind())).collect::<Vec<_>>(),
        }),
        Action::Other {
            selector, target, ..
        } => json!({
            "selector": selector,
            "target":   target.0,
        }),
    }
}

fn amount_json(a: &AmountSpec) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("tokenSymbol".into(), Value::from(a.token.symbol.clone()));
    m.insert("raw".into(), Value::from(a.raw.clone()));
    if let Some(h) = &a.human {
        m.insert("human".into(), Value::from(h.clone()));
    }
    if let Some(u) = &a.usd {
        m.insert(
            "usd".into(),
            json!({
                "value": { "__extn": { "fn": "decimal", "arg": u.value } },
                "staleSec": u.stale_sec as i64,
            }),
        );
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiply_decimal_strings_basic() {
        assert_eq!(multiply_decimal_strings("200000000", 6, "1.00"), "200.0000");
    }

    #[test]
    fn multiply_decimal_strings_weth_at_3000() {
        assert_eq!(
            multiply_decimal_strings("1000000000000000000", 18, "3000.0000"),
            "3000.0000"
        );
    }

    #[test]
    fn multiply_decimal_strings_fractional_token() {
        assert_eq!(
            multiply_decimal_strings("500000000000000000", 18, "3000.00"),
            "1500.0000"
        );
    }

    #[test]
    fn decimal_to_fixed_pads_short_fraction() {
        assert_eq!(decimal_to_fixed("1.5", 4), 15000);
        assert_eq!(decimal_to_fixed("1", 4), 10000);
        assert_eq!(decimal_to_fixed("0", 4), 0);
    }

    #[test]
    fn decimal_to_fixed_truncates_long_fraction() {
        assert_eq!(decimal_to_fixed("1.123456", 4), 11234);
    }
}
