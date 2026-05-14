use crate::action::Address;
use serde_json::{json, Value};

pub(crate) fn cedar_long_u64(value: u64) -> Value {
    let narrowed = i64::try_from(value).unwrap_or(i64::MAX);
    debug_assert!(
        i64::try_from(value).is_ok() || cfg!(test),
        "cedar Long narrowing clamped u64 value {value} to i64::MAX"
    );
    Value::from(narrowed)
}

/// Build the standard `Wallet` + `Protocol` entity bag for a policy request.
///
/// `from` becomes the `Wallet` uid; `to` (the transaction target contract)
/// becomes the `Protocol` uid so policies can write
/// `resource == Protocol::"0x..."` against the contract address.
pub(crate) fn entities(from: &Address, to: &Address) -> Value {
    let wallet_id = from.to_string();
    let protocol_id = to.to_string();
    json!([
        {
            "uid": { "type": "Wallet", "id": wallet_id.as_str() },
            "attrs": { "address": wallet_id.as_str() },
            "parents": []
        },
        {
            "uid": { "type": "Protocol", "id": protocol_id.as_str() },
            "attrs": {},
            "parents": []
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cedar_long_u64_clamps_values_above_i64_max() {
        assert_eq!(cedar_long_u64(42), json!(42));
        assert_eq!(cedar_long_u64(u64::MAX), json!(i64::MAX));
    }
}
