//! Shared Cedar JSON serialization for amount-shaped context fields.

use crate::action::{AmountConstraint, AmountKind, UsdValuation as ActionUsdValuation};
use crate::context_keys::VALUE;
use crate::core::UsdValuation;
use serde_json::{Map, Value};

pub(crate) fn usd_valuation_json(valuation: &UsdValuation) -> Value {
    crate::cedar_json::usd_valuation_json(valuation)
}

pub(crate) fn action_usd_valuation_json(
    valuation: &ActionUsdValuation,
    block_timestamp: u64,
) -> Value {
    usd_valuation_json(&UsdValuation {
        value: valuation.value.clone(),
        as_of_ts: valuation.as_of_ts.unwrap_or(block_timestamp),
        sources: valuation.sources.clone().unwrap_or_default(),
        stale_sec: valuation.stale_sec.unwrap_or_default(),
    })
}

pub(crate) fn amount_constraint_json(amount: &AmountConstraint) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), Value::from(amount_kind_str(&amount.kind)));
    if let Some(value) = &amount.value {
        out.insert(VALUE.into(), Value::from(value.to_string()));
    }
    Value::Object(out)
}

pub(crate) const fn amount_kind_str(kind: &AmountKind) -> &'static str {
    match kind {
        AmountKind::Exact => "exact",
        AmountKind::Min => "min",
        AmountKind::Max => "max",
        AmountKind::Unlimited => "unlimited",
        AmountKind::Estimated => "estimated",
        AmountKind::Unknown => "unknown",
    }
}
