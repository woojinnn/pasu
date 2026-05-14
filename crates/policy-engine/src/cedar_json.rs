//! Shared Cedar JSON encoding helpers.

use crate::context_keys::{
    AS_OF_TS, EXTN_ARG, EXTN_DECIMAL, EXTN_FN, EXTN_KEY, SOURCES, STALE_SEC, VALUE,
};
use crate::core::UsdValuation;
use serde_json::{Map, Value};

/// Encode a Cedar `decimal` extension value.
#[must_use]
pub fn decimal_json(value: &str) -> Value {
    let mut extension = Map::new();
    extension.insert(EXTN_FN.into(), Value::from(EXTN_DECIMAL));
    extension.insert(EXTN_ARG.into(), Value::from(value));

    let mut out = Map::new();
    out.insert(EXTN_KEY.into(), Value::Object(extension));
    Value::Object(out)
}

/// Encode a `u64` as Cedar `Long`, clamping values outside `i64`.
#[must_use]
pub fn long_u64_json(value: u64) -> Value {
    let narrowed = i64::try_from(value).unwrap_or(i64::MAX);
    debug_assert!(
        i64::try_from(value).is_ok() || cfg!(test),
        "Cedar Long narrowing clamped u64 value {value} to i64::MAX"
    );
    Value::from(narrowed)
}

/// Encode a policy-engine USD valuation as Cedar context JSON.
#[must_use]
pub fn usd_valuation_json(valuation: &UsdValuation) -> Value {
    let mut out = Map::new();
    out.insert(VALUE.into(), decimal_json(&valuation.value));
    out.insert(AS_OF_TS.into(), long_u64_json(valuation.as_of_ts));
    out.insert(STALE_SEC.into(), long_u64_json(valuation.stale_sec));
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
