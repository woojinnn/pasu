//! Independent per-selector projection oracle (PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK §1.2).
//!
//! A projection is a 2nd-opinion on decode **correctness** that does NOT trust
//! the production decoder. For one selector it re-derives the expected
//! `ActionBody` field values straight from the raw calldata (an *independent*
//! alloy ABI decode) plus tx context, then asserts them against the routed
//! envelope. Where [`semantic`](super::semantic) `expect_body` pins literal
//! values a human curated per tx, a projection **computes** the expected value
//! from `$raw.<arg>` / `$tx.*` / `$derive.*`, so authoring scales with the
//! selector count (not the tx count) and it catches the silent MIS-DECODED
//! class (the §9.4 dogfood) without hand-writing each expected value.
//!
//! ## Non-circularity (the load-bearing rule)
//! A projection may read the ABI signature, the raw calldata, and tx fields
//! ONLY. It must never read `emit.body`, manifest placeholders, or the decoder
//! output as its expected source — otherwise it is a duplicate decode smoke
//! test, not an independent check. This module enforces that structurally: the
//! expected side is built solely from an alloy decode of the calldata, never
//! from the envelope.
//!
//! Implemented: the data contract, the independent decode, the `$tx`/`$raw`/
//! `$derive` source grammar, and evaluation (reusing the [`semantic`] op +
//! JSON-path engine). The selector→tx matching + CLI/corpus driver that feeds
//! real envelopes in bulk is the remaining wiring (see `PROTOCOL_AGNOSTIC §1.2`).

use std::collections::BTreeMap;

use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use serde::Deserialize;
use serde_json::Value;

use super::semantic::{check_expect_body, AssertionOp, BodyAssertion};

/// One selector-level projection spec (`projections/<selector-or-name>.json`).
#[derive(Clone, Debug, Deserialize)]
pub struct ProjectionSpec {
    /// `0x`+8hex 4-byte selector this projection applies to.
    pub selector: String,
    /// Canonical function signature for the independent ABI decode, e.g.
    /// `"approve(address spender,uint256 amount)"`. Parameter names back the
    /// `$raw.<name>` grammar.
    pub signature: String,
    /// Optional chain/address scope (advisory; the caller usually pre-matches).
    #[serde(default)]
    pub scope: Scope,
    /// Field expectations.
    pub expect: Vec<ProjExpect>,
}

/// Chain/address scope for a projection.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Scope {
    /// Chain ids this projection applies to (empty = any).
    #[serde(default)]
    pub chains: Vec<u64>,
    /// Lowercase `to` addresses, or `["*"]` for any (empty = any).
    #[serde(default)]
    pub addresses: Vec<String>,
}

/// One projected field expectation. Either `from` (a source-grammar reference,
/// computed independently) or the literal `value`/`values` is the expected side;
/// `op` + `path` reuse the [`semantic`] assertion engine against the envelope.
#[derive(Clone, Debug, Deserialize)]
pub struct ProjExpect {
    /// JSON path into the routed envelope (same dialect as `expect_body`).
    pub path: String,
    /// Comparison operator (reused from [`semantic`]).
    pub op: AssertionOp,
    /// Source-grammar reference for the expected value (`$tx.*` / `$raw.*` /
    /// `$derive.*`). When present it overrides `value`.
    #[serde(default)]
    pub from: Option<String>,
    /// Literal expected value (used when `from` is absent).
    #[serde(default)]
    pub value: Value,
    /// Literal accepted values for `one_of`.
    #[serde(default)]
    pub values: Vec<Value>,
}

/// Transaction context exposed to the `$tx.*` source grammar.
#[derive(Clone, Copy, Debug)]
pub struct TxContext<'a> {
    /// EVM chain id.
    pub chain_id: u64,
    /// `to` address (lowercase `0x...`).
    pub to: &'a str,
    /// `from` address (lowercase `0x...`).
    pub from: &'a str,
    /// `value` in wei (decimal string).
    pub value: &'a str,
}

/// Independently decoded calldata: raw argument values keyed by parameter name.
/// (Projection signatures are authored with named params, per the `$raw.<arg>`
/// grammar; an unnamed param is simply unaddressable.)
struct RawArgs {
    by_name: BTreeMap<String, Value>,
}

/// Evaluate a projection against the routed envelope. Returns `Ok(())` when
/// every expectation holds, or `Err(mismatches)` (one string per failed
/// expectation) — a non-empty result is a `mis_decoded` finding, not a flake.
pub fn evaluate(
    spec: &ProjectionSpec,
    calldata_hex: &str,
    tx: &TxContext<'_>,
    envelope: &Value,
) -> Result<(), Vec<String>> {
    let raw = match decode_args(&spec.signature, calldata_hex) {
        Ok(raw) => raw,
        Err(e) => return Err(vec![format!("independent decode failed: {e}")]),
    };

    let mut failures = Vec::new();
    for (i, exp) in spec.expect.iter().enumerate() {
        let assertion = match build_assertion(exp, &raw, tx) {
            Ok(a) => a,
            Err(e) => {
                failures.push(format!("expect[{i}] {}: source error: {e}", exp.path));
                continue;
            }
        };
        if let Err(e) = check_expect_body(envelope, &[assertion]) {
            failures.push(format!("expect[{i}]: {e}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// Build a [`semantic::BodyAssertion`] whose expected side is resolved from the
/// projection source grammar (or the literal value).
fn build_assertion(
    exp: &ProjExpect,
    raw: &RawArgs,
    tx: &TxContext<'_>,
) -> Result<BodyAssertion, String> {
    // Ops that take no expected value pass through unchanged.
    let needs_value = !matches!(
        exp.op,
        AssertionOp::Exists | AssertionOp::Absent | AssertionOp::NonzeroAddress
    );
    let (value, values) = match &exp.from {
        Some(src) if needs_value => (resolve_source(src, raw, tx)?, Vec::new()),
        _ => (exp.value.clone(), exp.values.clone()),
    };
    Ok(BodyAssertion {
        path: exp.path.clone(),
        op: exp.op,
        value,
        values,
    })
}

/// Resolve a source-grammar reference to a concrete JSON value, reading ONLY
/// independent inputs (raw calldata decode + tx fields). Never the envelope.
fn resolve_source(src: &str, raw: &RawArgs, tx: &TxContext<'_>) -> Result<Value, String> {
    let src = src.trim();
    if let Some(field) = src.strip_prefix("$tx.") {
        return match field {
            "chain_id" => Ok(Value::from(tx.chain_id)),
            "to" => Ok(Value::from(tx.to)),
            "from" => Ok(Value::from(tx.from)),
            "value" => Ok(Value::from(tx.value)),
            _ => Err(format!("unknown $tx field `{field}`")),
        };
    }
    if let Some(rest) = src.strip_prefix("$raw.") {
        return resolve_raw(rest, raw);
    }
    if let Some(rest) = src.strip_prefix("$derive.") {
        return resolve_derive(rest, raw, tx);
    }
    Err(format!(
        "unsupported source `{src}` (expected $tx.* / $raw.* / $derive.*)"
    ))
}

/// `<name>` or `<name>[i]` against the decoded args.
fn resolve_raw(rest: &str, raw: &RawArgs) -> Result<Value, String> {
    if let Some((name, idx)) = parse_indexed(rest) {
        let base = raw
            .by_name
            .get(name)
            .ok_or_else(|| format!("$raw.{name}: no such argument"))?;
        return base
            .as_array()
            .and_then(|a| a.get(idx))
            .cloned()
            .ok_or_else(|| format!("$raw.{name}[{idx}]: not an array index"));
    }
    raw.by_name.get(rest).cloned().ok_or_else(|| {
        format!(
            "$raw.{rest}: no such argument (have: {:?})",
            raw.by_name.keys().collect::<Vec<_>>()
        )
    })
}

/// `<fn>(<inner-source>)` — harness-owned independent derivations.
fn resolve_derive(rest: &str, raw: &RawArgs, tx: &TxContext<'_>) -> Result<Value, String> {
    let (func, arg) = rest
        .split_once('(')
        .and_then(|(f, a)| a.strip_suffix(')').map(|a| (f.trim(), a.trim())))
        .ok_or_else(|| format!("malformed $derive.{rest} (expected fn(arg))"))?;
    // The inner argument is itself a source reference (usually $raw.path).
    let inner = resolve_source(arg, raw, tx)?;
    let hex = inner
        .as_str()
        .ok_or_else(|| format!("$derive.{func}: argument must resolve to a hex string"))?;
    match func {
        "lower_hex" => Ok(Value::from(hex.to_ascii_lowercase())),
        "uniswap_v3_path_first_token" => v3_path_token(hex, true),
        "uniswap_v3_path_last_token" => v3_path_token(hex, false),
        "uniswap_v3_path_first_fee" => v3_path_first_fee(hex),
        _ => Err(format!(
            "unknown $derive function `{func}` (extend the catalog deliberately)"
        )),
    }
}

/// Uniswap V3 path = token(20) [fee(3) token(20)]+ . First/last 20-byte token.
fn v3_path_token(hex: &str, first: bool) -> Result<Value, String> {
    let bytes = decode_hex(hex)?;
    if bytes.len() < 20 {
        return Err(format!("v3 path too short ({} bytes)", bytes.len()));
    }
    let slice = if first {
        &bytes[..20]
    } else {
        &bytes[bytes.len() - 20..]
    };
    Ok(Value::from(format!("0x{}", hex::encode(slice))))
}

/// First fee tier (3 bytes after the first 20-byte token) as a number.
fn v3_path_first_fee(hex: &str) -> Result<Value, String> {
    let bytes = decode_hex(hex)?;
    if bytes.len() < 23 {
        return Err(format!(
            "v3 path too short for a fee ({} bytes)",
            bytes.len()
        ));
    }
    let fee = (u32::from(bytes[20]) << 16) | (u32::from(bytes[21]) << 8) | u32::from(bytes[22]);
    Ok(Value::from(fee))
}

/// `name[i]` -> `(name, i)`; else `None`.
fn parse_indexed(s: &str) -> Option<(&str, usize)> {
    let (name, rest) = s.split_once('[')?;
    let idx = rest.strip_suffix(']')?.parse::<usize>().ok()?;
    Some((name, idx))
}

/// Independently decode `calldata_hex` against `signature` via alloy (NOT the
/// production decoder), returning named + positional raw arg values.
fn decode_args(signature: &str, calldata_hex: &str) -> Result<RawArgs, String> {
    let bytes = decode_hex(calldata_hex)?;
    if bytes.len() < 4 {
        return Err("calldata shorter than a 4-byte selector".to_owned());
    }
    let func =
        Function::parse(signature).map_err(|e| format!("parse signature `{signature}`: {e}"))?;
    let decoded = func
        .abi_decode_input(&bytes[4..], true)
        .map_err(|e| format!("abi_decode_input: {e}"))?;
    let mut by_name = BTreeMap::new();
    for (i, value) in decoded.iter().enumerate() {
        if let Some(param) = func.inputs.get(i) {
            if !param.name.is_empty() {
                by_name.insert(param.name.clone(), dyn_to_json(value));
            }
        }
    }
    Ok(RawArgs { by_name })
}

/// Map an alloy `DynSolValue` to the JSON shape the [`semantic`] ops compare
/// against. Addresses/bytes -> lowercase `0x` hex; integers -> decimal string
/// (so `u256_hex_eq` normalizes either side); tuples/arrays -> positional array.
fn dyn_to_json(value: &DynSolValue) -> Value {
    match value {
        DynSolValue::Address(a) => {
            Value::from(format!("0x{}", hex::encode(a.0 .0)).to_ascii_lowercase())
        }
        DynSolValue::Bool(b) => Value::from(*b),
        DynSolValue::Uint(u, _) => Value::from(u.to_string()),
        DynSolValue::Int(i, _) => Value::from(i.to_string()),
        DynSolValue::Bytes(b) => Value::from(format!("0x{}", hex::encode(b))),
        DynSolValue::FixedBytes(b, n) => Value::from(format!("0x{}", hex::encode(&b.0[..*n]))),
        DynSolValue::String(s) => Value::from(s.clone()),
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) | DynSolValue::Tuple(items) => {
            Value::Array(items.iter().map(dyn_to_json).collect())
        }
        other => Value::from(format!("{other:?}")),
    }
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let s = s
        .trim()
        .strip_prefix("0x")
        .or_else(|| s.trim().strip_prefix("0X"))
        .unwrap_or(s.trim());
    hex::decode(s).map_err(|e| format!("invalid hex: {e}"))
}

#[cfg(test)]
mod tests {
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::{Address, U256};
    use serde_json::json;

    use super::{evaluate, ProjectionSpec, TxContext};

    /// Build real `approve(spender, amount)` calldata via an independent encode.
    fn approve_calldata(spender: &str, amount: u128) -> String {
        let func = Function::parse("approve(address spender,uint256 amount)").unwrap();
        let args = vec![
            DynSolValue::Address(spender.parse::<Address>().unwrap()),
            DynSolValue::Uint(U256::from(amount), 256),
        ];
        let encoded = func.abi_encode_input(&args).unwrap();
        format!("0x{}", hex::encode(encoded))
    }

    fn approve_projection() -> ProjectionSpec {
        serde_json::from_value(json!({
            "selector": "0x095ea7b3",
            "signature": "approve(address spender,uint256 amount)",
            "expect": [
                { "path": "$..spender", "op": "hex_eq", "from": "$raw.spender" },
                { "path": "$..amount", "op": "u256_hex_eq", "from": "$raw.amount" },
                { "path": "$..token", "op": "hex_eq", "from": "$tx.to" }
            ]
        }))
        .unwrap()
    }

    const SPENDER: &str = "0x1111111111111111111111111111111111111111";
    const TOKEN: &str = "0x2222222222222222222222222222222222222222";

    fn tx() -> TxContext<'static> {
        TxContext {
            chain_id: 1,
            to: TOKEN,
            from: "0x9999999999999999999999999999999999999999",
            value: "0",
        }
    }

    #[test]
    fn projection_passes_on_a_faithful_decode() {
        let calldata = approve_calldata(SPENDER, 5000);
        // Envelope mimicking a CORRECT production decode of the approve.
        let envelope = json!({
            "ok": true,
            "data": { "actions": [{ "body": {
                "domain": "permission",
                "token": TOKEN,
                "spender": SPENDER,
                "amount": "0x1388"  // 5000, hex — u256_hex_eq normalizes vs decimal $raw.amount
            }}]}
        });
        evaluate(&approve_projection(), &calldata, &tx(), &envelope)
            .expect("faithful decode should pass");
    }

    #[test]
    fn projection_catches_a_silent_mis_decode() {
        // §9.4 class: the decode is well-shaped + valid-looking, but a field is
        // WRONG (spender swapped). The shape/domain oracle would pass it; the
        // projection must catch it because $raw.spender != envelope spender.
        let calldata = approve_calldata(SPENDER, 5000);
        let envelope = json!({
            "ok": true,
            "data": { "actions": [{ "body": {
                "domain": "permission",
                "token": TOKEN,
                "spender": "0xdeaddeaddeaddeaddeaddeaddeaddeaddeaddead",
                "amount": "0x1388"
            }}]}
        });
        let err = evaluate(&approve_projection(), &calldata, &tx(), &envelope)
            .expect_err("mis-decoded spender must be caught");
        assert!(
            err.iter().any(|e| e.contains("spender")),
            "expected a spender mismatch, got {err:?}"
        );
    }

    #[test]
    fn projection_catches_a_wrong_amount() {
        let calldata = approve_calldata(SPENDER, 5000);
        let envelope = json!({
            "ok": true,
            "data": { "actions": [{ "body": {
                "domain": "permission", "token": TOKEN, "spender": SPENDER, "amount": "0x270f" // 9999 != 5000
            }}]}
        });
        let err = evaluate(&approve_projection(), &calldata, &tx(), &envelope)
            .expect_err("wrong amount caught");
        assert!(
            err.iter().any(|e| e.contains("amount")),
            "expected amount mismatch, got {err:?}"
        );
    }

    #[test]
    fn derive_uniswap_v3_path_first_and_last_token() {
        // path = tokenA(20) | fee(3) | tokenB(20)
        let token_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let token_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let path = format!("0x{token_a}0001f4{token_b}");
        let calldata = {
            let func = Function::parse("exactInput(bytes path)").unwrap();
            let encoded = func
                .abi_encode_input(&[DynSolValue::Bytes(hex::decode(&path[2..]).unwrap())])
                .unwrap();
            format!("0x{}", hex::encode(encoded))
        };
        let spec: ProjectionSpec = serde_json::from_value(json!({
            "selector": "0xc04b8d59",
            "signature": "exactInput(bytes path)",
            "expect": [
                { "path": "$..token_in", "op": "hex_eq", "from": "$derive.uniswap_v3_path_first_token($raw.path)" },
                { "path": "$..token_out", "op": "hex_eq", "from": "$derive.uniswap_v3_path_last_token($raw.path)" },
                { "path": "$..fee", "op": "equals", "from": "$derive.uniswap_v3_path_first_fee($raw.path)" }
            ]
        }))
        .unwrap();
        let envelope = json!({
            "ok": true,
            "data": { "actions": [{ "body": {
                "token_in": format!("0x{token_a}"),
                "token_out": format!("0x{token_b}"),
                "fee": 500
            }}]}
        });
        evaluate(&spec, &calldata, &tx(), &envelope).expect("v3 path derivations should match");
    }
}
