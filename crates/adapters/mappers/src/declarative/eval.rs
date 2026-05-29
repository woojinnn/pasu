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

use std::str::FromStr as _;

use abi_resolver::{DecodedCall, DecodedValue};
use alloy_primitives::{I256, U256};

use crate::mapper::{MapContext, MapperError};

use super::builtin_fn;
use super::types::{BuiltinFn, ValueExpr};

/// Convert a single [`DecodedValue`] into a `serde_json::Value` view, WITHOUT
/// ABI-width context.
///
/// This is the type-agnostic entry point — `Uint` always renders as a decimal
/// string. Callers that have the ABI type string (e.g. [`args_to_json`] via
/// `DecodedArg::abi_type`) should prefer [`decoded_value_to_json_typed`] so
/// narrow uints (`uint8` .. `uint64`) render as JSON numbers (see that fn's
/// docs for the rationale). Kept for callers that carry no per-value type
/// (e.g. the enum-tagged bridge), where the historic decimal-string behaviour
/// is preserved unchanged.
pub fn decoded_value_to_json(value: &DecodedValue) -> serde_json::Value {
    decoded_value_to_json_typed(value, "")
}

/// Convert a [`DecodedValue`] into a `serde_json::Value` using the ABI type
/// string `abi_type` to pick the JSON encoding for integers.
///
/// Encoding rules:
///   * `Address` → JSON string `"0x.."` (lowercased by `Address::to_string`).
///   * `Uint` → JSON **number** when `abi_type` is a scalar `uintN` with
///     `N <= 64` (the value provably fits in `u64`); otherwise a JSON decimal
///     **string**. This mirrors serde: `u8` / `u16` / `u32` / `u64` struct
///     fields parse from a JSON number and REJECT a string, whereas `U256`
///     fields parse from a decimal string and lose precision as a JS number
///     (> 2^53). The width comes from the ABI param type, the only place it is
///     available — `DynSolValue::Uint` keeps the bit width but the
///     `DecodedValue::Uint(U256)` shape drops it, so we thread the type string.
///   * `Int` → JSON decimal string (unchanged). No narrow **signed** struct
///     field is in scope today; if one lands, apply the analogous `iN <= 64`
///     rule here.
///   * `Bool` → JSON boolean.
///   * `Bytes` → JSON string `"0x.." + hex`.
///   * `String` → JSON string.
///   * `Array` → JSON array; each element re-typed with the element type
///     (one trailing `[..]` group stripped from `abi_type`).
///   * `Tuple` → JSON array; when `abi_type` is a parenthesised tuple type its
///     top-level components are matched positionally, otherwise (e.g. the bare
///     `"tuple"` alloy emits, whose field types live in `components` not the
///     type string) each element falls back to the type-agnostic string form.
pub fn decoded_value_to_json_typed(value: &DecodedValue, abi_type: &str) -> serde_json::Value {
    match value {
        DecodedValue::Address(address) => serde_json::Value::String(address.to_string()),
        DecodedValue::Uint(value) => uint_to_json(*value, abi_type),
        DecodedValue::Int(value) => serde_json::Value::String(i256_to_decimal_string(*value)),
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
        // Pass the arg's ABI type so narrow uints (`uint8` .. `uint64`) render
        // as JSON numbers — `u8` / `u64` struct fields (Aave `categoryId`,
        // Permit2 `expiration` uint48) parse from numbers and reject strings.
        obj.insert(
            arg.name.clone(),
            decoded_value_to_json_typed(&arg.value, &arg.abi_type),
        );
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
            if let Some(capability) = via {
                return Err(MapperError::Unsupported(format!(
                    "FromArg.via:{capability}"
                )));
            }
            // `kind` is metadata for amount typing — interpreter ignores it
            // here, since the calling field already carries `.amount.kind` as a
            // separate fields entry. We only validate it parses if present.
            let _ = kind;

            evaluate_json_path(_ctx, args_json, from)
        }

        ValueExpr::Transform { function, args } => {
            evaluate_transform(_ctx, args_json, *function, args)
        }
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
            // Phase 7B (T-B3) — `unfold_v3_path` returns either a JSON
            // string (addresses) or a JSON number (fees) depending on
            // `select`. The interpreter is agnostic to the return shape;
            // downstream `single_emit` field builders coerce per-field.
            builtin_fn::unfold_v3_path(&bytes_value, select)
                .map_err(|error| MapperError::Internal(anyhow::anyhow!(error)))
        }
        BuiltinFn::CurveRouteLastToken => {
            // Phase 12.3, F3 + F-route1.B Tier B fix (V3 round, Phase C).
            // Curve Router NG output-token resolver.
            // 2 args:
            //   [0] `route:        address[11]`     (`$.args._route`)
            //   [1] `swap_params:  uint256[N][5]`   (`$.args._swap_params`)
            // Returns a JSON string (lowercased `0x..` address), shape-
            // compatible with `single_emit` `.asset.address` consumers.
            //
            // Pre-fix the resolver took only `route`, which meant swap_type=4/5/8/9
            // hops (LP_ADD / WRAPPED_ASSET_CONVERT / ERC4626_ASSET_SHARE) silently
            // misdecoded the output asset. `swap_params` is now required so the
            // per-hop convention (`route[2i+2]` coin vs `route[2i+1]` pool/helper/
            // vault) can be applied correctly.
            if args.len() != 2 {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "curve_route_last_token expects 2 args (route, swap_params), got {}",
                    args.len()
                )));
            }
            let route_value = evaluate(ctx, args_json, &args[0])?;
            let swap_params_value = evaluate(ctx, args_json, &args[1])?;
            builtin_fn::curve_route_last_token(&route_value, &swap_params_value)
                .map_err(|error| MapperError::Internal(anyhow::anyhow!(error)))
        }
        BuiltinFn::SelectFromLiteralArray => {
            // Phase 12.7 (P0-2) — pick `coins[i]` / `coins[j]` from a
            // pool-hardcoded literal array. 2 args: array (typically a
            // `{ "literal": [...] }` of token addresses) + int index
            // (typically `{ "from": "$.args.i" }`).
            if args.len() != 2 {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "select_from_literal_array expects 2 args, got {}",
                    args.len()
                )));
            }
            let array_value = evaluate(ctx, args_json, &args[0])?;
            let idx_value = evaluate(ctx, args_json, &args[1])?;
            builtin_fn::select_from_literal_array(&array_value, &idx_value)
                .map_err(|error| MapperError::Internal(anyhow::anyhow!(error)))
        }
        BuiltinFn::UnfoldSlipstreamPath => {
            // Phase 8 (Aerodrome CL) — args[0] = bytes, args[1] = select,
            // args[2] (optional) = hop_index for `tick_spacing_at_hop`.
            if !(2..=3).contains(&args.len()) {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "unfold_slipstream_path expects 2 or 3 args, got {}",
                    args.len()
                )));
            }
            let bytes_value = evaluate(ctx, args_json, &args[0])?;
            let select_value = evaluate(ctx, args_json, &args[1])?;
            let select = select_value.as_str().ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "unfold_slipstream_path: select must be string literal, got {select_value}"
                ))
            })?;
            let extra_value = if args.len() == 3 {
                Some(evaluate(ctx, args_json, &args[2])?)
            } else {
                None
            };
            // Slipstream returns JSON string for token addresses, JSON
            // number (signed i64) for tick spacings. Downstream
            // `single_emit` field builders coerce per-field.
            builtin_fn::unfold_slipstream_path(&bytes_value, select, extra_value.as_ref())
                .map_err(|error| MapperError::Internal(anyhow::anyhow!(error)))
        }
        BuiltinFn::UnfoldVeloV2Path => {
            // Phase 2 (Aerodrome UR V2_SWAP) — args[0] = packed V2 path
            // bytes, args[1] = select literal (`first_token` /
            // `last_token`). The path's first/last 20 bytes are always a
            // token address regardless of the UniV2 vs VeloV2 stable-byte
            // stride, so the built-in returns a JSON string (lowercase
            // `0x..` address). Downstream `single_emit` `.asset.address`
            // consumers take the string as-is.
            if args.len() != 2 {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "unfold_velo_v2_path expects 2 args, got {}",
                    args.len()
                )));
            }
            let bytes_value = evaluate(ctx, args_json, &args[0])?;
            let select_value = evaluate(ctx, args_json, &args[1])?;
            let select = select_value.as_str().ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "unfold_velo_v2_path: select must be string literal, got {select_value}"
                ))
            })?;
            builtin_fn::unfold_velo_v2_path(&bytes_value, select)
                .map_err(|error| MapperError::Internal(anyhow::anyhow!(error)))
        }
        BuiltinFn::MapRecipient => {
            // Phase F3 — resolve a UR/V4 action recipient sentinel
            // (`0x..01` → ctx.from, `0x..02` → ctx.to). 1 arg: the raw
            // recipient address, typically `{ "from": "$.args.recipient" }`.
            if args.len() != 1 {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "map_recipient expects 1 arg, got {}",
                    args.len()
                )));
            }
            let raw = evaluate(ctx, args_json, &args[0])?;
            let addr_str = raw.as_str().ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "map_recipient: arg must be an address string, got {raw}"
                ))
            })?;
            let addr = policy_engine::action::Address::from_str(addr_str).map_err(|message| {
                MapperError::Internal(anyhow::anyhow!(
                    "map_recipient: invalid address {addr_str:?}: {message}"
                ))
            })?;
            let mapped = crate::protocols::universal_router::common::map_recipient(ctx, addr);
            Ok(serde_json::Value::String(mapped.to_string()))
        }
        other => Err(MapperError::Unsupported(format!("builtin_fn/{other:?}"))),
    }
}

/// PoC JsonPath walker — supports:
///   * `$.args.<name>`
///   * `$.args.<name>[<idx>]`
///   * `$.args.<name>[<idx>][<idx>]...` (chained indices, Phase 5 — needed for
///     UR `PERMIT2_PERMIT.permitSingle[0][0]` style nested tuple access)
///   * `$.tx.<field>` — host tx metadata (Phase 7B): `value_wei`, `from`,
///     `to`, `chain_id`, `block_timestamp`. All synthesized as JSON strings
///     (decimal for numeric, lowercase `0x..` for addresses) to mirror the
///     `decoded_value_to_json` encoding.
///   * `$.context.<field>` — host recursion handles (Phase 7B):
///     `parent_calldata` (hex `0x..`), `depth` (decimal string). None values
///     materialize as empty string so policies can detect them explicitly.
///
/// We intentionally avoid pulling in a full JSONPath library. Each fixture's
/// queries reduce to "look up a named arg, then optionally index into nested
/// arrays/tuples". Dotted nested object access (`$.args.x.y`) is not supported
/// — call sites that need named-field access through a tuple should rely on
/// the Tier B JSON ABI bridge to expose top-level args, or use numeric tuple
/// indices.
fn evaluate_json_path(
    ctx: &MapContext<'_>,
    args_json: &serde_json::Value,
    path: &str,
) -> Result<serde_json::Value, MapperError> {
    // Strip the `$.` root marker and identify which root (`args` / `tx` /
    // `context`) the path targets. Each root has its own walker — `args`
    // delegates to the recursive JSON walker, `tx` / `context` resolve a
    // single field from `MapContext`.
    let body = path.strip_prefix("$.").ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "JsonPath must start with \"$.\", got {path:?}"
        ))
    })?;

    let (root, rest) = match body.find('.') {
        Some(dot) => (&body[..dot], &body[dot + 1..]),
        None => (body, ""),
    };

    match root {
        "args" => {
            if rest.is_empty() {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "JsonPath {path:?}: $.args requires a field name"
                )));
            }
            walk_args(args_json, rest, path).cloned()
        }
        "tx" => evaluate_tx_field(ctx, rest, path),
        "context" => evaluate_context_field(ctx, rest, path),
        other => Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: unknown root segment {other:?} (allowed: args, tx, context)"
        ))),
    }
}

/// Walk a chain of `name[idx][idx]...` segments under the `$.args` root.
///
/// Returns a borrowed reference into `args_json` so the caller can decide
/// whether to clone. Indices may be negative (counted from the array end);
/// out-of-range indices yield [`MapperError::Internal`].
fn walk_args<'a>(
    args_json: &'a serde_json::Value,
    rest: &str,
    path: &str,
) -> Result<&'a serde_json::Value, MapperError> {
    // Split off the leading name from any chain of `[idx]` suffixes.
    let (name, mut remainder) = match rest.find('[') {
        Some(open) => (&rest[..open], &rest[open..]),
        None => (rest, ""),
    };

    if name.is_empty() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: empty argument name"
        )));
    }

    let mut value = args_json
        .get(name)
        .ok_or_else(|| MapperError::MissingArgument(format!("$.args.{name} (path: {path})")))?;

    // Walk the chain of [idx] segments left-to-right.
    while !remainder.is_empty() {
        let open = remainder.find('[').ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "JsonPath {path:?}: unexpected remainder {remainder:?} (expected '[')"
            ))
        })?;
        if open != 0 {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "JsonPath {path:?}: characters between indices not supported (got \
                 {:?} before next '[')",
                &remainder[..open]
            )));
        }
        let after = &remainder[1..];
        let close = after.find(']').ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "JsonPath {path:?}: unterminated [..] index"
            ))
        })?;
        let idx_str = &after[..close];
        let trailing = &after[close + 1..];

        let idx: i64 = idx_str.parse().map_err(|_| {
            MapperError::Internal(anyhow::anyhow!(
                "JsonPath {path:?}: invalid index {idx_str:?}"
            ))
        })?;
        let arr = value.as_array().ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "JsonPath {path:?}: indexed access on non-array at remainder {remainder:?}"
            ))
        })?;
        // Round 1 audit (P1) — `-idx` for `idx == i64::MIN` is undefined in
        // two's complement (the positive counterpart is unrepresentable).
        // `checked_neg` returns `None` in that case so we fall through to a
        // bounds error instead of relying on wrapping semantics that vary by
        // build profile.
        let resolved_opt: Option<usize> = if idx >= 0 {
            usize::try_from(idx).ok().filter(|i| *i < arr.len())
        } else {
            idx.checked_neg()
                .and_then(|neg| usize::try_from(neg).ok())
                .filter(|abs| *abs <= arr.len())
                .map(|abs| arr.len() - abs)
        };
        let resolved = resolved_opt.ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "JsonPath {path:?}: index {idx} out of bounds (len={})",
                arr.len()
            ))
        })?;

        value = &arr[resolved];
        remainder = trailing;
    }

    Ok(value)
}

/// Resolve a `$.tx.<field>` JsonPath against the host [`MapContext`].
///
/// All values are encoded as JSON strings — decimal for `uint`-typed fields,
/// lowercase `0x..` for addresses — matching how `decoded_value_to_json`
/// encodes argument primitives. `block_timestamp` is `Option<u64>`; a missing
/// timestamp materializes as `""` so a policy can detect "no host clock" via
/// a string-equality check rather than a typed null.
fn evaluate_tx_field(
    ctx: &MapContext<'_>,
    rest: &str,
    path: &str,
) -> Result<serde_json::Value, MapperError> {
    // Phase 7B does not expose nested tx fields, so reject any `[..]` index
    // or `tx.x.y` chain. Future extensions can replace this with a walker.
    if rest.is_empty() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: $.tx requires a field name"
        )));
    }
    if rest.contains('.') || rest.contains('[') {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: $.tx fields do not support nested access"
        )));
    }

    match rest {
        "value_wei" => Ok(serde_json::Value::String(ctx.value_wei.to_string())),
        "from" => Ok(serde_json::Value::String(ctx.from.to_string())),
        "to" => Ok(serde_json::Value::String(ctx.to.to_string())),
        "chain_id" => Ok(serde_json::Value::String(ctx.chain_id.to_string())),
        "block_timestamp" => Ok(serde_json::Value::String(
            ctx.block_timestamp
                .map(|t| t.to_string())
                .unwrap_or_default(),
        )),
        other => Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: unknown $.tx field {other:?} \
             (allowed: value_wei, from, to, chain_id, block_timestamp)"
        ))),
    }
}

/// Resolve a `$.context.<field>` JsonPath against the host [`MapContext`].
///
/// Mirrors [`evaluate_tx_field`] but for recursion-specific handles
/// introduced in Phase 4 (`parent_calldata`, `depth`). `parent_calldata`
/// is `None` at the top level — we surface that as the empty string so
/// the same encoding choice as `block_timestamp` applies (callers can
/// branch on `== ""`).
fn evaluate_context_field(
    ctx: &MapContext<'_>,
    rest: &str,
    path: &str,
) -> Result<serde_json::Value, MapperError> {
    if rest.is_empty() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: $.context requires a field name"
        )));
    }
    if rest.contains('.') || rest.contains('[') {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: $.context fields do not support nested access"
        )));
    }

    match rest {
        "parent_calldata" => Ok(serde_json::Value::String(
            ctx.parent_calldata
                .map(|bytes| format!("0x{}", hex::encode(bytes)))
                .unwrap_or_default(),
        )),
        "depth" => Ok(serde_json::Value::String(ctx.depth.to_string())),
        other => Err(MapperError::Internal(anyhow::anyhow!(
            "JsonPath {path:?}: unknown $.context field {other:?} \
             (allowed: parent_calldata, depth)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_resolver::{DecodedArg, DecoderId};
    use policy_engine::action::Address;
    use serde_json::json;

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
            parent_calldata: None,
            depth: 0,
            resolver: None,
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
                            Address::from_str("0x1111111111111111111111111111111111111111")
                                .unwrap(),
                        ),
                        DecodedValue::Address(
                            Address::from_str("0x2222222222222222222222222222222222222222")
                                .unwrap(),
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
        let expr: ValueExpr = serde_json::from_value(json!({ "from": "$.args.path[0]" })).unwrap();
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

    // ── $.tx.* / $.context.* JsonPath (Phase 7B / T-B3) ──────────────────
    //
    // Each test builds a `MapContext` with a specific field value, feeds an
    // empty `args_json`, and asserts the JSON encoding matches the
    // documented contract (decimal strings for u64, lowercase hex for
    // addresses, `0x..` hex for byte slices, `""` for None).

    fn empty_args() -> serde_json::Value {
        serde_json::Value::Object(serde_json::Map::new())
    }

    fn eval_path_with_ctx<'a>(ctx: &MapContext<'a>, path: &str) -> serde_json::Value {
        let expr: ValueExpr = serde_json::from_value(json!({ "from": path })).unwrap();
        evaluate(ctx, &empty_args(), &expr).unwrap()
    }

    #[test]
    fn evaluate_tx_value_wei_returns_decimal_string() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("1000000000000000000").unwrap();
        let registry = EmptyTokenRegistry;
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        assert_eq!(
            eval_path_with_ctx(&ctx, "$.tx.value_wei"),
            json!("1000000000000000000")
        );
    }

    #[test]
    fn evaluate_tx_from_returns_lowercase_address() {
        let from = Address::from_str("0xAaBbCcDdEeFf00112233445566778899aAbBcCdD").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        // `Address::from_str` lowercases on parse, so the JSON encoding must
        // also be lowercase regardless of the input casing.
        assert_eq!(
            eval_path_with_ctx(&ctx, "$.tx.from"),
            json!("0xaabbccddeeff00112233445566778899aabbccdd")
        );
    }

    #[test]
    fn evaluate_tx_to_returns_lowercase_address() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        assert_eq!(
            eval_path_with_ctx(&ctx, "$.tx.to"),
            json!("0x1111111111111111111111111111111111111111")
        );
    }

    #[test]
    fn evaluate_tx_chain_id_returns_u64_string() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        assert_eq!(eval_path_with_ctx(&ctx, "$.tx.chain_id"), json!("1"));
    }

    #[test]
    fn evaluate_tx_block_timestamp_returns_decimal_string() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        // dummy_ctx pins block_timestamp = Some(1_700_000_000).
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        assert_eq!(
            eval_path_with_ctx(&ctx, "$.tx.block_timestamp"),
            json!("1700000000")
        );
    }

    #[test]
    fn evaluate_tx_block_timestamp_none_returns_empty_string() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let mut ctx = dummy_ctx(1, &from, &to, &value, &registry);
        ctx.block_timestamp = None;
        assert_eq!(eval_path_with_ctx(&ctx, "$.tx.block_timestamp"), json!(""));
    }

    #[test]
    fn evaluate_context_depth_returns_u8_string() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        // depth defaults to 0 in `dummy_ctx`.
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        assert_eq!(eval_path_with_ctx(&ctx, "$.context.depth"), json!("0"));
    }

    #[test]
    fn evaluate_context_parent_calldata_hex_encoded() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let mut ctx = dummy_ctx(1, &from, &to, &value, &registry);
        let parent_bytes: &[u8] = &[0xab, 0xcd];
        ctx.parent_calldata = Some(parent_bytes);
        assert_eq!(
            eval_path_with_ctx(&ctx, "$.context.parent_calldata"),
            json!("0xabcd")
        );
    }

    #[test]
    fn evaluate_context_parent_calldata_none_returns_empty_string() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        // dummy_ctx leaves parent_calldata=None.
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        assert_eq!(
            eval_path_with_ctx(&ctx, "$.context.parent_calldata"),
            json!("")
        );
    }

    #[test]
    fn evaluate_unknown_tx_field_errors() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        let expr: ValueExpr = serde_json::from_value(json!({ "from": "$.tx.nope" })).unwrap();
        let err = evaluate(&ctx, &empty_args(), &expr).unwrap_err();
        assert!(matches!(err, MapperError::Internal(_)), "got {err:?}");
    }

    #[test]
    fn evaluate_unknown_root_errors() {
        let from = Address::from_str("0x000000000000000000000000000000000000abcd").unwrap();
        let to = Address::from_str("0x000000000000000000000000000000000000beef").unwrap();
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let registry = EmptyTokenRegistry;
        let ctx = dummy_ctx(1, &from, &to, &value, &registry);
        // `$.host.x` is intentionally rejected — only `args`, `tx`, `context`
        // are wired in Phase 7B.
        let expr: ValueExpr = serde_json::from_value(json!({ "from": "$.host.x" })).unwrap();
        let err = evaluate(&ctx, &empty_args(), &expr).unwrap_err();
        assert!(matches!(err, MapperError::Internal(_)), "got {err:?}");
    }
}
