//! `$fn` call-form executors for the V3 `emit.body` DSL.
//!
//! The V3 placeholder walker
//! ([`substitute_placeholders`](super::action_builder::substitute_placeholders))
//! resolves a `{ "$fn": "<name>", "$args": [<arg templates>] }` object by
//! substituting each arg, then dispatching here **by name**. These are the
//! WhitelistedFn backends (§5.3.2 / [`super::types::BuiltinFn`]) that a single
//! `$args.x` reference or a `$match` value-map cannot express — notably Curve
//! Router NG's variable-hop output token, which depends on a per-hop swap-type.
//!
//! Each executor is JSON-in / JSON-out: args arrive **already substituted**
//! (addresses as lowercase `"0x"` hex strings, uints as decimal strings or JSON
//! numbers, per [`super::args_json`]), and the returned JSON is spliced back
//! into the `emit.body` template by the caller.

use std::str::FromStr as _;

use alloy_primitives::{keccak256, Address};
use serde_json::Value as JsonValue;

/// The whitelist of accepted `$fn` names. The author-time validator
/// (`registryV2/scripts/build-index.ts` `validateEmitShape`) mirrors this so an
/// unknown `$fn` fails at build-index time, not only at decode time.
pub const WHITELIST: &[&str] = &["curve_route_last_token", "route_hash"];

/// Dispatch a `$fn` call by name against its already-substituted JSON args.
///
/// Returns `Err(reason)` for an unknown function, a wrong arg count, or a
/// malformed/invalid argument; the caller wraps it in
/// [`V3BuildError::FnCall`](super::action_builder::V3BuildError::FnCall).
pub fn dispatch(name: &str, args: &[JsonValue]) -> Result<JsonValue, String> {
    match name {
        "curve_route_last_token" => curve_route_last_token(args),
        "route_hash" => route_hash(args),
        _ => Err(format!(
            "unknown $fn '{name}' (whitelist: {})",
            WHITELIST.join(", ")
        )),
    }
}

/// `curve_route_last_token(route: address[11], swap_params: uint256[N][5]) -> address`.
///
/// Mirrors Curve `Router.vy::exchange` per-hop semantics: hop `i` executes while
/// the pool slot `route[2i+1]` is non-zero. The output token of the **last**
/// executed hop is `route[2i+2]` for coin-producing swap types (1/2/3/6/7) or
/// `route[2i+1]` for pool/LP/vault-producing swap types (4 `LP_ADD`,
/// 5 `LENDING_TO_LP`, 8 `WRAPPED_ASSET_CONVERT`, 9 `ERC4626_ASSET_SHARE`).
/// `swap_type` is read from `swap_params[i][2]` (the inner index is `[2]` for
/// both the `uint256[5][5]` and `uint256[4][5]` Router NG variants).
fn curve_route_last_token(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "curve_route_last_token expects 2 args (route, swap_params), got {}",
            args.len()
        ));
    }
    let route = args[0]
        .as_array()
        .ok_or("curve_route_last_token: route arg is not an array")?;
    let swap_params = args[1]
        .as_array()
        .ok_or("curve_route_last_token: swap_params arg is not an array")?;

    // Last executed hop = highest `i` whose pool slot route[2i+1] is non-zero.
    // Router NG packs at most 5 hops into the 11-element route array.
    let mut last_hop: Option<usize> = None;
    for i in 0..5usize {
        let pool_idx = 2 * i + 1; // 1, 3, 5, 7, 9
        match route.get(pool_idx) {
            Some(v) if !is_zero_address(v) => last_hop = Some(i),
            _ => break,
        }
    }
    let i = last_hop.ok_or("curve_route_last_token: empty route (no non-zero pool slot)")?;

    let swap_type_val = swap_params
        .get(i)
        .and_then(JsonValue::as_array)
        .and_then(|inner| inner.get(2))
        .ok_or_else(|| format!("curve_route_last_token: missing swap_params[{i}][2]"))?;
    // A real Router NG swap_type is a small integer in 1..=9. A value that does
    // not fit u64 (a fuzzed uint256) or is out of that enum is unreachable on a
    // real route — fold both into one "unknown swap_type" (soft under fuzzing,
    // hard-asserted against by the corpus/golden).
    let out_idx = match json_to_u64(swap_type_val) {
        Some(1 | 2 | 3 | 6 | 7) => 2 * i + 2, // coin-producing → next coin slot
        Some(4 | 5 | 8 | 9) => 2 * i + 1,     // pool/LP/vault is itself the output
        _ => {
            return Err(format!(
                "curve_route_last_token: unknown swap_type {swap_type_val} at hop {i}"
            ))
        }
    };
    route.get(out_idx).cloned().ok_or_else(|| {
        format!(
            "curve_route_last_token: route[{out_idx}] out of bounds (len {})",
            route.len()
        )
    })
}

/// `route_hash(route: address[11]) -> bytes32` — a deterministic ScopeBall
/// identity for an aggregator route (NOT an on-chain value). Defined as
/// `keccak256(route[0] ++ route[1] ++ … )` over the packed 20-byte addresses,
/// so the same structural route hashes identically regardless of amounts. Feeds
/// `AmmVenue::AggregatorRoute.route_hash` ("32-byte hex hash of the route").
fn route_hash(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "route_hash expects 1 arg (route), got {}",
            args.len()
        ));
    }
    let route = args[0]
        .as_array()
        .ok_or("route_hash: route arg is not an array")?;
    let mut bytes = Vec::with_capacity(route.len() * 20);
    for (idx, v) in route.iter().enumerate() {
        let s = v
            .as_str()
            .ok_or_else(|| format!("route_hash: route[{idx}] is not a string address"))?;
        let addr = Address::from_str(s)
            .map_err(|e| format!("route_hash: route[{idx}] is not an address ({s}): {e}"))?;
        bytes.extend_from_slice(addr.as_slice());
    }
    Ok(JsonValue::String(format!("{:#x}", keccak256(&bytes))))
}

/// A route slot is "empty" when it parses to the zero address (Router NG
/// zero-pads unused hops). Falls back to an all-zero-hex string check if the
/// value is not a parseable address.
fn is_zero_address(v: &JsonValue) -> bool {
    match v.as_str() {
        Some(s) => Address::from_str(s).map_or_else(
            |_| s.trim_start_matches("0x").chars().all(|c| c == '0'),
            |a| a.is_zero(),
        ),
        None => false,
    }
}

/// Read a `uint` arg as `u64` — decimal string (width > 64) or JSON number
/// (width ≤ 64), matching how [`super::args_json`] renders uints.
fn json_to_u64(v: &JsonValue) -> Option<u64> {
    match v {
        JsonValue::Number(n) => n.as_u64(),
        JsonValue::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "0x1111111111111111111111111111111111111111";
    const POOL1: &str = "0x2222222222222222222222222222222222222222";
    const B: &str = "0x3333333333333333333333333333333333333333";
    const POOL2: &str = "0x4444444444444444444444444444444444444444";
    const C: &str = "0x5555555555555555555555555555555555555555";
    const Z: &str = "0x0000000000000000000000000000000000000000";

    fn route(addrs: &[&str]) -> JsonValue {
        let mut v: Vec<JsonValue> = addrs.iter().map(|s| json!(s)).collect();
        while v.len() < 11 {
            v.push(json!(Z));
        }
        JsonValue::Array(v)
    }

    #[test]
    fn single_hop_coin_swap_emits_next_coin() {
        // 1 hop, swap_type 1 (coin-producing) → route[2] = B.
        let r = route(&[A, POOL1, B]);
        let sp = json!([["0", "1", "1", "1", "2"]]); // swap_type at [0][2] = "1"
        let out = curve_route_last_token(&[r, sp]).unwrap();
        assert_eq!(out, json!(B));
    }

    #[test]
    fn two_hop_uses_last_hop_output() {
        // 2 hops, both coin-producing → last output route[4] = C.
        let r = route(&[A, POOL1, B, POOL2, C]);
        let sp = json!([["0", "1", "1", "1", "2"], ["0", "1", "1", "1", "2"]]);
        let out = curve_route_last_token(&[r, sp]).unwrap();
        assert_eq!(out, json!(C));
    }

    #[test]
    fn vault_swap_type_emits_pool_slot() {
        // swap_type 9 (ERC4626 share) → output is the pool/vault slot route[1].
        let r = route(&[A, POOL1]);
        let sp = json!([[0, 0, 9, 0, 0]]); // numeric swap_type accepted too
        let out = curve_route_last_token(&[r, sp]).unwrap();
        assert_eq!(out, json!(POOL1));
    }

    #[test]
    fn empty_route_errors() {
        let r = route(&[A]); // route[1] is zero → no executed hop
        let sp = json!([["0", "0", "1", "0", "0"]]);
        assert!(curve_route_last_token(&[r, sp]).is_err());
    }

    #[test]
    fn route_hash_is_deterministic_and_32_bytes() {
        let r = route(&[A, POOL1, B]);
        let h1 = route_hash(std::slice::from_ref(&r)).unwrap();
        let h2 = route_hash(&[r]).unwrap();
        assert_eq!(h1, h2);
        let s = h1.as_str().unwrap();
        assert!(s.starts_with("0x"));
        assert_eq!(s.len(), 66); // 0x + 64 hex
                                 // different route → different hash
        let other = route_hash(&[route(&[A, POOL1, C])]).unwrap();
        assert_ne!(h1, other);
    }

    #[test]
    fn dispatch_unknown_fn_errors() {
        assert!(dispatch("nope", &[]).is_err());
    }
}
