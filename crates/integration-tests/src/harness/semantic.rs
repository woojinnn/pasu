//! Protocol-agnostic semantic assertions for corpus replay.
//!
//! This module intentionally knows nothing about concrete protocols or
//! `ActionBody` variants. It checks JSON paths against the routed envelope so
//! curated corpus entries can pin decoded fields without adding one-off Rust
//! tests per protocol.

use alloy_primitives::U256;
use serde::Deserialize;
use serde_json::Value;

/// One field-level assertion attached to a corpus transaction.
#[derive(Clone, Debug, Deserialize)]
pub struct BodyAssertion {
    /// JSON path to inspect.
    pub path: String,
    /// Assertion operator.
    pub op: AssertionOp,
    /// Single expected value for unary comparisons.
    #[serde(default)]
    pub value: Value,
    /// Accepted values for `one_of`.
    #[serde(default)]
    pub values: Vec<Value>,
}

/// Supported protocol-agnostic assertion operators.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssertionOp {
    /// Path must resolve to at least one value.
    Exists,
    /// Path must resolve to no values.
    Absent,
    /// Resolved value must equal `value`.
    Equals,
    /// Resolved value must not equal `value`.
    NotEquals,
    /// Resolved value must equal one of `values`.
    OneOf,
    /// Resolved array/string must contain `value`.
    Contains,
    /// Resolved array/string/object length must equal numeric `value`.
    Len,
    /// Resolved string must be a non-zero EVM address.
    NonzeroAddress,
    /// Resolved string and `value` must be equal 0x-prefixed hex, ignoring case.
    HexEq,
    /// Resolved value and `value` must be equal as decimal/hex U256 quantities.
    U256HexEq,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PathToken {
    Field(String),
    Index(usize),
}

/// Check all field-level assertions against a routed envelope.
pub fn check_expect_body(envelope: &Value, assertions: &[BodyAssertion]) -> Result<(), String> {
    for assertion in assertions {
        check_assertion(envelope, assertion)?;
    }
    Ok(())
}

fn check_assertion(envelope: &Value, assertion: &BodyAssertion) -> Result<(), String> {
    let values = values_at_path(envelope, &assertion.path)?;
    if assertion.op == AssertionOp::Absent {
        return if values.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} absent failed: resolved {} value(s)",
                assertion.path,
                values.len()
            ))
        };
    }
    if values.is_empty() {
        return Err(format!(
            "{} {:?} failed: path missing",
            assertion.path, assertion.op
        ));
    }

    let mut first_err = None;
    let actual_debug = values_debug(&values);
    for actual in &values {
        match matches_assertion(actual, assertion) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(e) => {
                first_err.get_or_insert(e);
            }
        };
    }

    Err(first_err.unwrap_or_else(|| {
        format!(
            "{} {:?} failed: expected {}, got {}",
            assertion.path,
            assertion.op,
            expectation_debug(assertion),
            actual_debug
        )
    }))
}

fn matches_assertion(actual: &Value, assertion: &BodyAssertion) -> Result<bool, String> {
    Ok(match assertion.op {
        AssertionOp::Exists => true,
        AssertionOp::Absent => false,
        AssertionOp::Equals => actual == &assertion.value,
        AssertionOp::NotEquals => actual != &assertion.value,
        AssertionOp::OneOf => assertion.values.iter().any(|v| actual == v),
        AssertionOp::Contains => contains_value(actual, &assertion.value)?,
        AssertionOp::Len => {
            len_of(actual).is_some_and(|len| Some(len) == expected_len(&assertion.value))
        }
        AssertionOp::NonzeroAddress => actual.as_str().is_some_and(is_nonzero_address),
        AssertionOp::HexEq => hex_literal(actual)? == hex_literal(&assertion.value)?,
        AssertionOp::U256HexEq => value_to_u256(actual)? == value_to_u256(&assertion.value)?,
    })
}

fn contains_value(actual: &Value, expected: &Value) -> Result<bool, String> {
    match actual {
        Value::Array(items) => Ok(items.iter().any(|item| item == expected)),
        Value::String(text) => expected
            .as_str()
            .map(|needle| text.contains(needle))
            .ok_or_else(|| "contains on string requires string `value`".to_owned()),
        _ => Err(format!("contains unsupported for {}", type_name(actual))),
    }
}

fn len_of(value: &Value) -> Option<usize> {
    match value {
        Value::Array(items) => Some(items.len()),
        Value::Object(map) => Some(map.len()),
        Value::String(text) => Some(text.len()),
        _ => None,
    }
}

fn expected_len(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok())
}

fn values_at_path<'a>(root: &'a Value, path: &str) -> Result<Vec<&'a Value>, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("empty assertion path".to_owned());
    }
    if path.starts_with('/') {
        return Ok(root.pointer(path).into_iter().collect());
    }
    if path == "$" {
        return Ok(vec![root]);
    }
    if let Some(field) = path.strip_prefix("$..") {
        if field.is_empty() || field.contains(['.', '[', ']']) {
            return Err(format!("unsupported recursive path `{path}`"));
        }
        let mut out = Vec::new();
        collect_recursive_field(root, field, &mut out);
        return Ok(out);
    }

    let rest = path
        .strip_prefix('$')
        .ok_or_else(|| format!("path must start with `$` or `/`: `{path}`"))?;
    let tokens = parse_path_tokens(rest)?;
    let mut current = vec![root];
    for token in tokens {
        let mut next = Vec::new();
        for value in current {
            match &token {
                PathToken::Field(name) => {
                    if let Some(child) = value.get(name.as_str()) {
                        next.push(child);
                    }
                }
                PathToken::Index(index) => {
                    if let Some(child) = value.as_array().and_then(|items| items.get(*index)) {
                        next.push(child);
                    }
                }
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }
    Ok(current)
}

fn parse_path_tokens(path: &str) -> Result<Vec<PathToken>, String> {
    let bytes = path.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                if start == i {
                    return Err(format!("empty field segment in path `${path}`"));
                }
                tokens.push(PathToken::Field(path[start..i].to_owned()));
            }
            b'[' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i == bytes.len() {
                    return Err(format!("unclosed index segment in path `${path}`"));
                }
                let index = path[start..i]
                    .parse::<usize>()
                    .map_err(|e| format!("invalid index in path `${path}`: {e}"))?;
                tokens.push(PathToken::Index(index));
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                tokens.push(PathToken::Field(path[start..i].to_owned()));
            }
        }
    }
    Ok(tokens)
}

fn collect_recursive_field<'a>(value: &'a Value, field: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if let Some(v) = map.get(field) {
                out.push(v);
            }
            for child in map.values() {
                collect_recursive_field(child, field, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_recursive_field(child, field, out);
            }
        }
        _ => {}
    }
}

fn is_nonzero_address(value: &str) -> bool {
    let Some(hex) = strip_hex_prefix(value.trim()) else {
        return false;
    };
    hex.len() == 40 && hex.chars().all(|c| c.is_ascii_hexdigit()) && hex.chars().any(|c| c != '0')
}

fn hex_literal(value: &Value) -> Result<String, String> {
    let text = value
        .as_str()
        .ok_or_else(|| format!("hex_eq requires string value, got {}", type_name(value)))?
        .trim();
    let Some(hex) = strip_hex_prefix(text) else {
        return Err(format!("hex_eq requires 0x-prefixed string: {text}"));
    };
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("invalid hex literal: {text}"));
    }
    Ok(format!("0x{}", hex.to_ascii_lowercase()))
}

fn value_to_u256(value: &Value) -> Result<U256, String> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .map(U256::from)
            .ok_or_else(|| format!("U256 assertion requires non-negative integer, got {n}")),
        Value::String(s) => parse_u256_quantity(s),
        _ => Err(format!(
            "U256 assertion requires string or integer, got {}",
            type_name(value)
        )),
    }
}

fn parse_u256_quantity(value: &str) -> Result<U256, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("empty U256 quantity".to_owned());
    }
    if let Some(hex) = strip_hex_prefix(value) {
        let hex = if hex.is_empty() { "0" } else { hex };
        return U256::from_str_radix(hex, 16).map_err(|e| format!("invalid U256 hex {value}: {e}"));
    }
    if !value.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid U256 decimal {value}"));
    }
    U256::from_str_radix(value, 10).map_err(|e| format!("invalid U256 decimal {value}: {e}"))
}

fn strip_hex_prefix(value: &str) -> Option<&str> {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn expectation_debug(assertion: &BodyAssertion) -> String {
    if assertion.op == AssertionOp::OneOf {
        return serde_json::to_string(&assertion.values).unwrap_or_else(|_| "<values>".to_owned());
    }
    serde_json::to_string(&assertion.value).unwrap_or_else(|_| "<value>".to_owned())
}

fn values_debug(values: &[&Value]) -> String {
    let mut parts = values
        .iter()
        .map(|value| short_json(value))
        .collect::<Vec<_>>();
    if parts.len() > 4 {
        parts.truncate(4);
        parts.push("...".to_owned());
    }
    format!("[{}]", parts.join(", "))
}

fn short_json(value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<value>".to_owned());
    const LIMIT: usize = 160;
    if text.len() <= LIMIT {
        text
    } else {
        let mut truncated = text.chars().take(LIMIT).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{check_expect_body, AssertionOp, BodyAssertion};

    fn assertion(path: &str, op: AssertionOp, value: serde_json::Value) -> BodyAssertion {
        BodyAssertion {
            path: path.to_owned(),
            op,
            value,
            values: Vec::new(),
        }
    }

    #[test]
    fn exact_path_equals() {
        let envelope = json!({
            "ok": true,
            "data": { "actions": [{ "body": { "domain": "amm", "action": "swap" } }] }
        });
        check_expect_body(
            &envelope,
            &[assertion(
                "$.data.actions[0].body.domain",
                AssertionOp::Equals,
                json!("amm"),
            )],
        )
        .unwrap();
    }

    #[test]
    fn recursive_path_nonzero_address() {
        let envelope = json!({
            "data": { "actions": [{ "body": {
                "token_in": { "key": { "address": "0x1111111111111111111111111111111111111111" } }
            }}]}
        });
        check_expect_body(
            &envelope,
            &[assertion(
                "$..address",
                AssertionOp::NonzeroAddress,
                Value::Null,
            )],
        )
        .unwrap();
    }

    #[test]
    fn absent_passes_only_when_path_missing() {
        let envelope = json!({ "data": { "actions": [] } });
        check_expect_body(
            &envelope,
            &[assertion(
                "$.data.actions[0]",
                AssertionOp::Absent,
                Value::Null,
            )],
        )
        .unwrap();
    }

    #[test]
    fn hex_eq_is_case_insensitive() {
        let envelope = json!({ "selector": "0xA9059CBB" });
        check_expect_body(
            &envelope,
            &[assertion(
                "$.selector",
                AssertionOp::HexEq,
                json!("0xa9059cbb"),
            )],
        )
        .unwrap();
    }

    #[test]
    fn u256_hex_eq_compares_decimal_and_hex() {
        let envelope = json!({ "value": "100000000000000" });
        check_expect_body(
            &envelope,
            &[assertion(
                "$.value",
                AssertionOp::U256HexEq,
                json!("0x5af3107a4000"),
            )],
        )
        .unwrap();
    }

    #[test]
    fn contains_and_len_work_for_generic_json() {
        let envelope = json!({ "items": ["token", "amount"] });
        check_expect_body(
            &envelope,
            &[
                assertion("$.items", AssertionOp::Contains, json!("amount")),
                assertion("$.items", AssertionOp::Len, json!(2)),
            ],
        )
        .unwrap();
    }
}
