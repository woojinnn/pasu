//! `ValueExpr` evaluator.
//!
//! Spec §5.1 BNF — `ValueExpr := Literal | FromArg | Transform`.
//!
//! Phase 1A simplifications:
//!   * JsonPath support is limited to `$.args.<name>` and `$.args.<name>[<idx>]`.
//!     This is sufficient for the four `single_emit` strategy fixtures
//!     (V2 swap, V3 swap, lending supply, weth wrap). A general JSONPath crate
//!     is intentionally avoided.
//!   * Only [`BuiltinFn::SelectAddress`] is wired to a backing function. Other
//!     `Transform` invocations return [`MapperError::Internal`].
//!   * `DecodedValue` is converted to `serde_json::Value` on entry so JsonPath
//!     traversal, `Literal` blending, and `builtin_fn::select_address` can all
//!     share one representation.

use abi_resolver::{DecodedCall, DecodedValue};
use alloy_primitives::{I256, U256};

use crate::mapper::{MapContext, MapperError};

use super::builtin_fn;
use super::types::{BuiltinFn, ValueExpr};

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
        DecodedValue::Bytes(bytes) => serde_json::Value::String(format!("0x{}", hex::encode(bytes))),
        DecodedValue::String(string) => serde_json::Value::String(string.clone()),
        DecodedValue::Array(values) | DecodedValue::Tuple(values) => serde_json::Value::Array(
            values.iter().map(decoded_value_to_json).collect(),
        ),
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
        obj.insert(arg.name.clone(), decoded_value_to_json(&arg.value));
    }
    serde_json::Value::Object(obj)
}

/// Evaluate a [`ValueExpr`] against the bundle's argument view.
///
/// `args_json` is the JSON object produced by [`args_to_json`] for the current
/// [`DecodedCall`]. We accept it pre-computed so the caller can build it once
/// per `map()` invocation instead of once per `ValueExpr`.
pub fn evaluate(
    _ctx: &MapContext<'_>,
    args_json: &serde_json::Value,
    expr: &ValueExpr,
) -> Result<serde_json::Value, MapperError> {
    match expr {
        ValueExpr::Literal { literal } => Ok(literal.clone()),

        ValueExpr::FromArg { from, via, kind } => {
            if via.is_some() {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "FromArg.via (host capability) is not implemented in Phase 1A: {via:?}"
                )));
            }
            // `kind` is metadata for amount typing — interpreter ignores it
            // here, since the calling field already carries `.amount.kind` as a
            // separate fields entry. We only validate it parses if present.
            let _ = kind;

            evaluate_json_path(args_json, from).cloned()
        }

        ValueExpr::Transform { function, args } => evaluate_transform(_ctx, args_json, *function, args),
    }
}

fn evaluate_transform(
    ctx: &MapContext<'_>,
    args_json: &serde_json::Value,
    function: BuiltinFn,
    args: &[ValueExpr],
) -> Result<serde_json::Value, MapperError> {
    match function {
        BuiltinFn::SelectAddress => {
            if args.len() != 2 {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "select_address expects 2 args, got {}",
                    args.len()
                )));
            }
            let arr = evaluate(ctx, args_json, &args[0])?;
            let idx_json = evaluate(ctx, args_json, &args[1])?;
            let idx = idx_json.as_i64().ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "select_address: idx must be integer, got {idx_json}"
                ))
            })?;
            let address = builtin_fn::select_address(&arr, idx)
                .map_err(|error| MapperError::Internal(anyhow::anyhow!(error)))?;
            Ok(serde_json::Value::String(address.to_string()))
        }
        BuiltinFn::UnfoldV3Path => {
            if args.len() != 2 {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "unfold_v3_path expects 2 args, got {}",
                    args.len()
                )));
            }
            let bytes_value = evaluate(ctx, args_json, &args[0])?;
            let select_value = evaluate(ctx, args_json, &args[1])?;
            let select = select_value.as_str().ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "unfold_v3_path: select must be string literal, got {select_value}"
                ))
            })?;
            let address = builtin_fn::unfold_v3_path(&bytes_value, select)
                .map_err(|error| MapperError::Internal(anyhow::anyhow!(error)))?;
            Ok(serde_json::Value::String(address.to_string()))
        }
        other => Err(MapperError::Internal(anyhow::anyhow!(
            "builtin {other:?} is not implemented in Phase 1A"
        ))),
    }
}

/// PoC JsonPath walker — supports `$.args.<name>` and `$.args.<name>[<idx>]`.
///
/// We intentionally avoid pulling in a full JSONPath library: each Tier A
/// fixture in Phase 1 uses one of these two shapes, and richer queries fall
/// into later phases (`unfold_v3_path`, multicall recurse, etc.).
fn evaluate_json_path<'a>(
    args_json: &'a serde_json::Value,
    path: &str,
) -> Result<&'a serde_json::Value, MapperError> {
    let rest = path
        .strip_prefix("$.args.")
        .ok_or_else(|| MapperError::Internal(anyhow::anyhow!(
            "Phase 1A JsonPath must start with \"$.args.\", got {path:?}"
        )))?;

    // Split off optional [idx] suffix.
    let (name, idx_part) = match rest.find('[') {
        Some(open) => {
            let after = &rest[open + 1..];
            let close = after.find(']').ok_or_else(|| MapperError::Internal(anyhow::anyhow!(
                "JsonPath {path:?}: unterminated [..] index"
            )))?;
            let name = &rest[..open];
            let trailing = &after[close + 1..];
            if !trailing.is_empty() {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "JsonPath {path:?}: trailing characters after closing ]"
                )));
            }
            let idx_str = &after[..close];
            (name, Some(idx_str))
        }
        None => (rest, None),
    };

    if name.is_empty() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: empty argument name"
        )));
    }

    let value = args_json.get(name).ok_or_else(|| {
        MapperError::MissingArgument(format!("$.args.{name} (path: {path})"))
    })?;

    let Some(idx_str) = idx_part else {
        return Ok(value);
    };

    let idx: i64 = idx_str.parse().map_err(|_| {
        MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: invalid index {idx_str:?}"
        ))
    })?;
    let arr = value.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: indexed access on non-array"
        ))
    })?;

    let resolved_opt: Option<usize> = if idx >= 0 {
        usize::try_from(idx).ok().filter(|i| *i < arr.len())
    } else {
        usize::try_from(-idx)
            .ok()
            .filter(|abs| *abs <= arr.len())
            .map(|abs| arr.len() - abs)
    };
    let resolved = resolved_opt.ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: index {idx} out of bounds (len={})",
            arr.len()
        ))
    })?;

    Ok(&arr[resolved])
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_resolver::{DecodedArg, DecoderId};
    use policy_engine::action::Address;
    use serde_json::json;
    use std::str::FromStr as _;

    use crate::token_registry::EmptyTokenRegistry;

    fn dummy_ctx<'a>(
        chain_id: u64,
        from: &'a Address,
        to: &'a Address,
        value: &'a policy_engine::action::DecimalString,
        registry: &'a EmptyTokenRegistry,
    ) -> MapContext<'a> {
        MapContext {
            chain_id,
            from,
            to,
            value_wei: value,
            block_timestamp: Some(1_700_000_000),
            token_registry: registry,
        }
    }

    fn sample_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new("test"),
            function_signature: "fn(uint256,address[])".into(),
            args: vec![
                DecodedArg {
                    name: "amountIn".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(1_000_u64)),
                },
                DecodedArg {
                    name: "path".into(),
                    abi_type: "address[]".into(),
                    value: DecodedValue::Array(vec![
                        DecodedValue::Address(
                            Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
                        ),
                        DecodedValue::Address(
                            Address::from_str("0x2222222222222222222222222222222222222222").unwrap(),
                        ),
                    ]),
                },
            ],
            nested: vec![],
        }
    }

    fn evaluate_with(decoded: &DecodedCall, expr: &ValueExpr) -> serde_json::Value {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        let args_json = args_to_json(decoded);
        evaluate(&ctx, &args_json, expr).unwrap()
    }

    #[test]
    fn evaluate_literal_returns_json_clone() {
        let decoded = sample_decoded();
        let expr: ValueExpr = serde_json::from_value(json!({ "literal": "erc20" })).unwrap();
        assert_eq!(evaluate_with(&decoded, &expr), json!("erc20"));
    }

    #[test]
    fn evaluate_from_arg_uint_returns_decimal_string() {
        let decoded = sample_decoded();
        let expr: ValueExpr = serde_json::from_value(json!({ "from": "$.args.amountIn" })).unwrap();
        assert_eq!(evaluate_with(&decoded, &expr), json!("1000"));
    }

    #[test]
    fn evaluate_from_arg_path_returns_array() {
        let decoded = sample_decoded();
        let expr: ValueExpr = serde_json::from_value(json!({ "from": "$.args.path" })).unwrap();
        assert_eq!(
            evaluate_with(&decoded, &expr),
            json!([
                "0x1111111111111111111111111111111111111111",
                "0x2222222222222222222222222222222222222222"
            ])
        );
    }

    #[test]
    fn evaluate_transform_select_address_first() {
        let decoded = sample_decoded();
        let expr: ValueExpr = serde_json::from_value(json!({
            "fn": "select_address",
            "args": [{ "from": "$.args.path" }, { "literal": 0 }]
        }))
        .unwrap();
        assert_eq!(
            evaluate_with(&decoded, &expr),
            json!("0x1111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn evaluate_transform_select_address_last() {
        let decoded = sample_decoded();
        let expr: ValueExpr = serde_json::from_value(json!({
            "fn": "select_address",
            "args": [{ "from": "$.args.path" }, { "literal": -1 }]
        }))
        .unwrap();
        assert_eq!(
            evaluate_with(&decoded, &expr),
            json!("0x2222222222222222222222222222222222222222")
        );
    }

    #[test]
    fn evaluate_json_path_with_index() {
        let decoded = sample_decoded();
        let expr: ValueExpr =
            serde_json::from_value(json!({ "from": "$.args.path[0]" })).unwrap();
        assert_eq!(
            evaluate_with(&decoded, &expr),
            json!("0x1111111111111111111111111111111111111111")
        );
    }

    /// Build a `DecodedCall` with a `path` arg containing a single-hop V3
    /// packed payload (`WETH --3000--> USDC`).
    fn v3_path_decoded() -> DecodedCall {
        let bytes = hex::decode(concat!(
            "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "000bb8",
            "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        ))
        .unwrap();
        DecodedCall {
            decoder_id: DecoderId::new("test"),
            function_signature: "exactInput((bytes,address,uint256,uint256,uint256))".into(),
            args: vec![DecodedArg {
                name: "path".into(),
                abi_type: "bytes".into(),
                value: DecodedValue::Bytes(bytes),
            }],
            nested: vec![],
        }
    }

    #[test]
    fn evaluate_transform_unfold_v3_path_first_token() {
        let decoded = v3_path_decoded();
        let expr: ValueExpr = serde_json::from_value(json!({
            "fn": "unfold_v3_path",
            "args": [
                { "from": "$.args.path" },
                { "literal": "first_token" }
            ]
        }))
        .unwrap();
        assert_eq!(
            evaluate_with(&decoded, &expr),
            json!("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")
        );
    }

    #[test]
    fn evaluate_transform_unfold_v3_path_last_token() {
        let decoded = v3_path_decoded();
        let expr: ValueExpr = serde_json::from_value(json!({
            "fn": "unfold_v3_path",
            "args": [
                { "from": "$.args.path" },
                { "literal": "last_token" }
            ]
        }))
        .unwrap();
        assert_eq!(
            evaluate_with(&decoded, &expr),
            json!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        );
    }
}
