//! v1-free JSON view of decoded calldata args.
//!
//! Extracted from the (deleted) v1 `eval.rs` so the v3 declarative route
//! (`policy-engine-wasm::declarative_exports::declarative_route_request_v3_json`)
//! can keep using `args_to_json` / `decoded_value_to_json` without dragging in
//! the v1 `Mapper` / `MapContext` machinery.
//!
//! These helpers depend only on `abi_resolver` (the decoded-value model),
//! `alloy_primitives`, `hex`, and `serde_json` — no `crate::mapper`.

use abi_resolver::{DecodedCall, DecodedValue};
use alloy_primitives::{I256, U256};

/// Convert a single [`DecodedValue`] into a `serde_json::Value` view.
///
/// Encoding rules:
///   * `Address` → JSON string `"0x.."` (lowercased by `Address::to_string`).
///   * `Uint` / `Int` → JSON string of the base-10 representation. This keeps
///     `uint256` values lossless (JS numbers lose precision beyond 2^53), and
///     matches how `DecimalString` parses.
///   * `Bool` → JSON boolean.
///   * `Bytes` → JSON string `"0x.." + hex`.
///   * `String` → JSON string.
///   * `Array` / `Tuple` → JSON array of recursively-encoded values.
pub fn decoded_value_to_json(value: &DecodedValue) -> serde_json::Value {
    match value {
        DecodedValue::Address(address) => serde_json::Value::String(address.to_string()),
        DecodedValue::Uint(value) => serde_json::Value::String(u256_to_decimal_string(*value)),
        DecodedValue::Int(value) => serde_json::Value::String(i256_to_decimal_string(*value)),
        DecodedValue::Bool(value) => serde_json::Value::Bool(*value),
        DecodedValue::Bytes(bytes) => {
            serde_json::Value::String(format!("0x{}", hex::encode(bytes)))
        }
        DecodedValue::String(string) => serde_json::Value::String(string.clone()),
        DecodedValue::Array(values) | DecodedValue::Tuple(values) => {
            serde_json::Value::Array(values.iter().map(decoded_value_to_json).collect())
        }
    }
}

fn u256_to_decimal_string(value: U256) -> String {
    value.to_string()
}

fn i256_to_decimal_string(value: I256) -> String {
    value.to_string()
}

/// Build the `args` map used by JsonPath evaluation.
///
/// Each [`abi_resolver::DecodedArg`] becomes one key in a JSON object, indexed
/// by argument name. This shape matches `$.args.<name>` selectors in the
/// bundle's `ValueExpr` entries.
#[must_use]
pub fn args_to_json(decoded: &DecodedCall) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for arg in &decoded.args {
        // Typed conversion: scalar `uint<=64` → JSON number (so ActionBody
        // `u8/u32/u64` fields — fee tiers, deadlines — deserialize), wider ints
        // → decimal string, with nested-tuple component types threaded.
        obj.insert(
            arg.name.clone(),
            decoded_value_to_json_typed(&arg.value, &arg.abi_type),
        );
    }
    serde_json::Value::Object(obj)
}

// ── typed-aware JSON conversion (ported from deleted eval.rs — uint≤64 number
//    coercion + nested-tuple component-type threading; powers Permit2/SR02 decode) ──
pub fn decoded_value_to_json_typed(value: &DecodedValue, abi_type: &str) -> serde_json::Value {
    match value {
        DecodedValue::Address(address) => serde_json::Value::String(address.to_string()),
        DecodedValue::Uint(value) => uint_to_json(*value, abi_type),
        DecodedValue::Int(value) => int_to_json(*value, abi_type),
        DecodedValue::Bool(value) => serde_json::Value::Bool(*value),
        DecodedValue::Bytes(bytes) => {
            serde_json::Value::String(format!("0x{}", hex::encode(bytes)))
        }
        DecodedValue::String(string) => serde_json::Value::String(string.clone()),
        DecodedValue::Array(values) => {
            let elem_type = array_element_type(abi_type).unwrap_or("");
            serde_json::Value::Array(
                values
                    .iter()
                    .map(|v| decoded_value_to_json_typed(v, elem_type))
                    .collect(),
            )
        }
        DecodedValue::Tuple(values) => {
            let component_types = tuple_component_types(abi_type);
            serde_json::Value::Array(
                values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let ty = component_types
                            .as_ref()
                            .and_then(|cs| cs.get(i))
                            .map_or("", String::as_str);
                        decoded_value_to_json_typed(v, ty)
                    })
                    .collect(),
            )
        }
    }
}

/// Encode a `U256` either as a JSON number (scalar `uintN`, `N <= 64`) or a
/// JSON decimal string (everything else). See [`decoded_value_to_json_typed`].
fn uint_to_json(value: U256, abi_type: &str) -> serde_json::Value {
    match uint_bits(abi_type) {
        // `N <= 64` ⇒ the value fits in `u64` by construction; the
        // `try_into` is a belt-and-braces guard — if it ever failed we fall
        // back to the lossless decimal string rather than truncating.
        Some(bits) if bits <= 64 => match u64::try_from(value) {
            Ok(n) => serde_json::Value::Number(serde_json::Number::from(n)),
            Err(_) => serde_json::Value::String(u256_to_decimal_string(value)),
        },
        _ => serde_json::Value::String(u256_to_decimal_string(value)),
    }
}

/// Parse the bit width of a SCALAR unsigned-integer ABI type string.
///
/// `"uint8"` → 8, `"uint48"` → 48, `"uint256"` → 256, bare `"uint"` → 256
/// (Solidity alias). Anything that is not a scalar uint (`"uint8[]"`,
/// `"tuple"`, `"address"`, `""`) → `None`. The trailing array/`[..]` suffix is
/// NOT stripped here — array element typing is handled by the caller, so a
/// type with a bracket is correctly rejected as "not a scalar uint".
fn uint_bits(abi_type: &str) -> Option<u32> {
    let t = abi_type.trim();
    let digits = t.strip_prefix("uint")?;
    if digits.is_empty() {
        return Some(256); // bare `uint` == `uint256`
    }
    // A scalar `uintN` is all-ASCII-digits after the prefix; a bracket or any
    // other char (array suffix, etc.) means this is not a scalar uint.
    if digits.bytes().all(|b| b.is_ascii_digit()) {
        digits.parse::<u32>().ok()
    } else {
        None
    }
}

/// Encode an `I256` either as a JSON number (scalar `intN`, `N <= 64`) or a
/// JSON decimal string (everything else). Mirror of [`uint_to_json`]; see
/// [`decoded_value_to_json_typed`] for the rationale.
fn int_to_json(value: I256, abi_type: &str) -> serde_json::Value {
    match int_bits(abi_type) {
        // `N <= 64` ⇒ the value fits in `i64` by construction; the `try_into`
        // is a belt-and-braces guard — on the (unreachable) failure path we
        // fall back to the lossless decimal string rather than truncating.
        Some(bits) if bits <= 64 => match i64::try_from(value) {
            Ok(n) => serde_json::Value::Number(serde_json::Number::from(n)),
            Err(_) => serde_json::Value::String(i256_to_decimal_string(value)),
        },
        _ => serde_json::Value::String(i256_to_decimal_string(value)),
    }
}

/// Parse the bit width of a SCALAR signed-integer ABI type string.
///
/// `"int8"` → 8, `"int24"` → 24, `"int256"` → 256, bare `"int"` → 256
/// (Solidity alias). Anything that is not a scalar int (`"int24[]"`, `"tuple"`,
/// `"uint256"`, `""`) → `None`. Mirrors [`uint_bits`]; the trailing array
/// suffix is NOT stripped here (array element typing is the caller's job).
fn int_bits(abi_type: &str) -> Option<u32> {
    let t = abi_type.trim();
    // `strip_prefix("int")` would also match the `int` inside `uint8` after a
    // `u`-strip elsewhere, but here `t` is the raw type — a uint starts with
    // `u`, so `strip_prefix("int")` correctly rejects it.
    let digits = t.strip_prefix("int")?;
    if digits.is_empty() {
        return Some(256); // bare `int` == `int256`
    }
    if digits.bytes().all(|b| b.is_ascii_digit()) {
        digits.parse::<u32>().ok()
    } else {
        None
    }
}

/// For an array ABI type, return the element type by stripping the single
/// trailing `[..]` (fixed or dynamic) group.
///
/// `"uint256[]"` → `"uint256"`, `"uint256[3]"` → `"uint256"`,
/// `"address[][2]"` → `"address[]"`. Returns `None` when `abi_type` is not an
/// array (no trailing `]`).
fn array_element_type(abi_type: &str) -> Option<&str> {
    let t = abi_type.trim_end();
    if !t.ends_with(']') {
        return None;
    }
    // Find the matching `[` for the final `]` (no nested brackets inside a
    // single dimension group, so the last `[` before the end is the match).
    let open = t.rfind('[')?;
    Some(&t[..open])
}

/// For a parenthesised tuple ABI type `"(t1,t2,..)"`, return its top-level
/// component type strings. Returns `None` for a non-tuple type (including the
/// bare `"tuple"` / `"tuple[]"` alloy emits when field types are carried out of
/// band on `components`).
fn tuple_component_types(abi_type: &str) -> Option<Vec<String>> {
    let t = abi_type.trim();
    let inner = t.strip_prefix('(')?.strip_suffix(')')?;
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(inner[start..i].trim().to_owned());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].trim().to_owned());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_resolver::{DecodedArg, DecoderId};

    fn sample_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new("test"),
            function_signature: "fn(uint256,bytes,bool)".into(),
            args: vec![
                DecodedArg {
                    name: "amountIn".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_u64)),
                },
                DecodedArg {
                    name: "path".into(),
                    abi_type: "bytes".into(),
                    value: DecodedValue::Bytes(vec![0xab, 0xcd]),
                },
                DecodedArg {
                    name: "flag".into(),
                    abi_type: "bool".into(),
                    value: DecodedValue::Bool(true),
                },
            ],
            nested: vec![],
        }
    }

    #[test]
    fn args_to_json_indexes_by_name() {
        let json = args_to_json(&sample_decoded());
        assert_eq!(json["amountIn"], serde_json::json!("1000"));
        assert_eq!(json["path"], serde_json::json!("0xabcd"));
        assert_eq!(json["flag"], serde_json::json!(true));
    }

    #[test]
    fn decoded_value_uint_is_decimal_string() {
        assert_eq!(
            decoded_value_to_json(&DecodedValue::Uint(U256::from(42_u64))),
            serde_json::json!("42")
        );
    }

    #[test]
    fn decoded_value_array_recurses() {
        let arr = DecodedValue::Array(vec![
            DecodedValue::Uint(U256::from(1_u64)),
            DecodedValue::Uint(U256::from(2_u64)),
        ]);
        assert_eq!(decoded_value_to_json(&arr), serde_json::json!(["1", "2"]));
    }
}
