//! Shared Cedar JSON serialization for amount-shaped context fields.

use crate::action::{AmountConstraint, AmountKind, UsdValuation as ActionUsdValuation};
use crate::context_keys::{
    AS_OF_TS, EXTN_ARG, EXTN_DECIMAL, EXTN_FN, EXTN_KEY, SOURCES, STALE_SEC, VALUE,
};
use crate::core::UsdValuation;
use serde_json::{Map, Value};

use super::cedar::cedar_long_u64;

pub(crate) fn decimal_json(value: &str) -> Value {
    let mut extension = Map::new();
    extension.insert(EXTN_FN.into(), Value::from(EXTN_DECIMAL));
    extension.insert(EXTN_ARG.into(), Value::from(value));

    let mut out = Map::new();
    out.insert(EXTN_KEY.into(), Value::Object(extension));
    Value::Object(out)
}

pub(crate) fn usd_valuation_json(valuation: &UsdValuation) -> Value {
    let mut out = Map::new();
    out.insert(VALUE.into(), decimal_json(&valuation.value));
    out.insert(AS_OF_TS.into(), cedar_long_u64(valuation.as_of_ts));
    out.insert(STALE_SEC.into(), cedar_long_u64(valuation.stale_sec));
    out.insert(
        SOURCES.into(),
        Value::Array(
            valuation
                .sources
                .iter()
                .map(|source| Value::from(source.as_str()))
                .collect(),
        ),
    );
    Value::Object(out)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decimal_json_uses_cedar_extension_keys() {
        assert_eq!(
            decimal_json("12.34"),
            json!({ "__extn": { "fn": "decimal", "arg": "12.34" } })
        );
    }
}
