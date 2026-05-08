//! Shared Cedar JSON serialization for amount-shaped context fields.

use crate::context_keys::{
    ADDRESS, AS_OF_TS, CHAIN_ID, DECIMALS, EXTN_ARG, EXTN_DECIMAL, EXTN_FN, EXTN_KEY, IS_NATIVE,
    SOURCES, STALE_SEC, SYMBOL, VALUE,
};
use crate::core::{Token, UsdValuation};
use serde_json::{Map, Value};

pub(super) fn decimal_json(value: &str) -> Value {
    let mut extension = Map::new();
    extension.insert(EXTN_FN.into(), Value::from(EXTN_DECIMAL));
    extension.insert(EXTN_ARG.into(), Value::from(value));

    let mut out = Map::new();
    out.insert(EXTN_KEY.into(), Value::Object(extension));
    Value::Object(out)
}

pub(super) fn token_json(token: &Token) -> Value {
    let mut out = Map::new();
    out.insert(CHAIN_ID.into(), Value::from(cedar_long_u64(token.chain_id)));
    out.insert(ADDRESS.into(), Value::from(token.address.as_str()));
    out.insert(SYMBOL.into(), Value::from(token.symbol.as_str()));
    out.insert(DECIMALS.into(), Value::from(i64::from(token.decimals)));
    out.insert(IS_NATIVE.into(), Value::from(token.is_native));
    Value::Object(out)
}

pub(super) fn usd_valuation_json(valuation: &UsdValuation) -> Value {
    let mut out = Map::new();
    out.insert(VALUE.into(), decimal_json(&valuation.value));
    out.insert(
        AS_OF_TS.into(),
        Value::from(cedar_long_u64(valuation.as_of_ts)),
    );
    out.insert(
        STALE_SEC.into(),
        Value::from(cedar_long_u64(valuation.stale_sec)),
    );
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

fn cedar_long_u64(value: u64) -> i64 {
    let narrowed = i64::try_from(value).unwrap_or(i64::MAX);
    debug_assert!(
        i64::try_from(value).is_ok() || cfg!(test),
        "cedar Long narrowing clamped u64 value {value} to i64::MAX"
    );
    narrowed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Address;
    use serde_json::json;

    #[test]
    fn decimal_json_uses_cedar_extension_keys() {
        assert_eq!(
            decimal_json("12.34"),
            json!({ "__extn": { "fn": "decimal", "arg": "12.34" } })
        );
    }

    #[test]
    fn token_json_uses_schema_field_names() {
        let token = Token {
            chain_id: 1,
            address: Address::new("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
            symbol: "USDT".into(),
            decimals: 6,
            is_native: false,
        };

        assert_eq!(
            token_json(&token),
            json!({
                "chainId": 1,
                "address": "0xdac17f958d2ee523a2206206994597c13d831ec7",
                "symbol": "USDT",
                "decimals": 6,
                "isNative": false,
            })
        );
    }
}
