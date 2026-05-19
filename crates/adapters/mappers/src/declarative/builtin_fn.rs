//! Built-in functions invoked by `ValueExpr::Transform`.
//!
//! Phase 1A: only `select_address` is implemented. Spec §5.3.1 ("WhitelistedFn").
//! Phase 3 adds `unfold_v3_path` (TierBBackedFn — backend wraps
//! [`abi_resolver::subdecode::protocols::uniswap_v3::decode_v3_path`]).
//!
//! All built-ins operate over `serde_json::Value` — the interpreter normalises
//! `DecodedValue` to JSON in [`super::eval`] and then walks JSON paths /
//! invokes built-ins on the JSON view. This keeps the interpreter generic
//! across argument types at the cost of one normalisation pass per call.

use std::str::FromStr as _;

use abi_resolver::subdecode::protocols::uniswap_v3::{decode_v3_path, PathDecodeError};
use policy_engine::action::Address;
use thiserror::Error;

/// Error variants for built-in evaluation.
#[derive(Debug, Error)]
pub enum FnError {
    #[error("select_address: argument is not an array (got {0})")]
    NotArray(&'static str),
    #[error("select_address: index out of bounds (idx={idx}, len={len})")]
    IndexOutOfBounds { idx: i64, len: usize },
    #[error("select_address: element {idx} is not an address (value={value})")]
    NotAddress {
        idx: i64,
        value: serde_json::Value,
    },
    #[error("select_address: invalid address {value}: {message}")]
    InvalidAddress { value: String, message: String },

    // ── unfold_v3_path ────────────────────────────────────────────────────
    /// `bytes` argument was neither a `"0x.."` hex string nor an array of
    /// numeric octets.
    #[error("unfold_v3_path: bytes argument has unsupported shape: {message}")]
    BytesShape { message: String },
    /// V3 packed path failed structural validation (length not `20 + 23*N`).
    #[error("unfold_v3_path: {error}")]
    PathDecode { error: PathDecodeError },
    /// `select` literal was not one of the four supported modes
    /// (`first_token`, `last_token`, `first_fee`, `last_fee`).
    #[error(
        "unfold_v3_path: unknown select {0:?} \
         (allowed: first_token, last_token, first_fee, last_fee)"
    )]
    UnknownSelect(String),
    /// `select` argument was not a JSON string.
    #[error("unfold_v3_path: select must be a string literal, got {0}")]
    SelectNotString(serde_json::Value),

    // ── curve_route_last_token (Phase 12.3) ──────────────────────────────
    /// Argument shape was not what the built-in expected (e.g. expected
    /// `address[]`, got scalar). Used by `curve_route_last_token`.
    #[error("type mismatch: expected {expected}, got {got}")]
    TypeMismatch {
        expected: &'static str,
        got: serde_json::Value,
    },
    /// Fixed-size array length disagreement. Used by `curve_route_last_token`,
    /// which requires exactly 11 slots.
    #[error("length mismatch: expected {expected}, got {got}")]
    LengthMismatch { expected: usize, got: usize },
    /// `curve_route_last_token` found an all-zero route (every even-index
    /// slot was `address(0)`), so no output token could be resolved.
    #[error("curve_route_last_token: route is empty (all token slots zero)")]
    EmptyRoute,

    // ── unfold_slipstream_path (Phase 8 — Aerodrome) ─────────────────────
    /// Slipstream packed path failed structural validation
    /// (length not `20 + 23 * N` for any `N >= 1`).
    #[error("unfold_slipstream_path: {message}")]
    SlipstreamPathDecode { message: String },
    /// `tick_spacing_at_hop` was called without a numeric `hop_index` arg.
    #[error("unfold_slipstream_path: tick_spacing_at_hop requires i64 hop_index arg")]
    SlipstreamHopIndexMissing,
}

/// `select_address(arr: address[], idx: i64) -> AddressRef` (spec §5.3.1).
///
/// `idx` semantics:
///   * `idx >= 0` — pick `arr[idx]`.
///   * `idx <  0` — pick `arr[arr.len() + idx]` (e.g. `-1` = last element).
///
/// Out-of-bounds yields [`FnError::IndexOutOfBounds`].
pub fn select_address(arr: &serde_json::Value, idx: i64) -> Result<Address, FnError> {
    let elements = arr.as_array().ok_or(FnError::NotArray(
        "select_address expects address[] (json array)",
    ))?;

    let len = elements.len();
    let resolved = resolve_index(idx, len)?;
    let element = &elements[resolved];

    let raw = element.as_str().ok_or_else(|| FnError::NotAddress {
        idx,
        value: element.clone(),
    })?;

    Address::from_str(raw).map_err(|message| FnError::InvalidAddress {
        value: raw.to_owned(),
        message,
    })
}

/// `unfold_v3_path(bytes: Bytes, select) -> AddressRef | u32`
/// (spec §5.3.2 — TierBBackedFn).
///
/// Backend wraps [`abi_resolver::subdecode::protocols::uniswap_v3::decode_v3_path`].
/// The packed-path format is `[token0(20B)][fee0(3B)][token1(20B)][fee1(3B)] ...`,
/// so token-endpoint and fee-endpoint selectors map directly onto the decoded
/// `(Vec<Address>, Vec<u32>)`.
///
/// Supported `select` modes:
///   * `"first_token"` / `"last_token"` — return JSON string containing the
///     lowercase `0x..` address (Phase 3).
///   * `"first_fee"` / `"last_fee"` — return JSON number with the uint24 fee
///     (Phase 7B / T-B3, e.g. `500` for the 0.05% tier).
///
/// `bytes_value` accepts either:
///   * JSON string `"0x.."` (the canonical encoding produced by
///     [`super::eval::decoded_value_to_json`] for `DecodedValue::Bytes`).
///   * JSON array of integers — each element must be in `0..=255`.
///
/// Anything else yields [`FnError::BytesShape`]. Decoding errors propagate as
/// [`FnError::PathDecode`].
pub fn unfold_v3_path(
    bytes_value: &serde_json::Value,
    select: &str,
) -> Result<serde_json::Value, FnError> {
    let bytes = json_value_to_bytes(bytes_value)?;
    let (tokens, fees) = decode_v3_path(&bytes).map_err(|error| FnError::PathDecode { error })?;

    match select {
        "first_token" => {
            let alloy_addr = *tokens
                .first()
                .expect("decode_v3_path guarantees tokens.len() >= 2 on success");
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "last_token" => {
            let alloy_addr = *tokens
                .last()
                .expect("decode_v3_path guarantees tokens.len() >= 2 on success");
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "first_fee" => {
            let fee = *fees
                .first()
                .expect("decode_v3_path guarantees fees.len() >= 1 on success");
            Ok(serde_json::Value::Number(serde_json::Number::from(fee)))
        }
        "last_fee" => {
            let fee = *fees
                .last()
                .expect("decode_v3_path guarantees fees.len() >= 1 on success");
            Ok(serde_json::Value::Number(serde_json::Number::from(fee)))
        }
        other => Err(FnError::UnknownSelect(other.to_owned())),
    }
}

/// `unfold_slipstream_path(bytes, select [, hop_index]) -> AddressRef | i32`
/// (Phase 8 — Aerodrome CL).
///
/// Slipstream path encoding mirrors the Uniswap V3 shape but the middle 3
/// bytes are a **signed** `int24 tickSpacing` instead of a `uint24 fee`:
///
/// ```text
/// [token0 (20)][tickSpacing0 (3)][token1 (20)][tickSpacing1 (3)] … [tokenN (20)]
/// ```
///
/// Sign-extension is required on decode (high bit of byte 0 → `0xFF` padding).
///
/// Supported `select` modes:
///   * `"first_token"` / `"last_token"` — JSON string of lowercase address
///   * `"first_tick_spacing"` / `"last_tick_spacing"` — JSON number (i64, sign-preserved)
///   * `"tick_spacing_at_hop"` — JSON number, requires `extra_arg` = i64 hop index
///                                (negative idx counts from the end, like `select_address`)
///
/// `bytes_value` accepts: hex string `"0x.."` OR JSON array of u8 (same as
/// [`unfold_v3_path`]).
pub fn unfold_slipstream_path(
    bytes_value: &serde_json::Value,
    select: &str,
    extra_arg: Option<&serde_json::Value>,
) -> Result<serde_json::Value, FnError> {
    let bytes = json_value_to_bytes(bytes_value)?;
    let (tokens, tick_spacings) = decode_slipstream_path(&bytes)?;
    match select {
        "first_token" => {
            let alloy_addr = *tokens
                .first()
                .expect("decode_slipstream_path guarantees tokens.len() >= 2 on success");
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "last_token" => {
            let alloy_addr = *tokens
                .last()
                .expect("decode_slipstream_path guarantees tokens.len() >= 2 on success");
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "first_tick_spacing" => {
            let ts = *tick_spacings
                .first()
                .expect("decode_slipstream_path guarantees tick_spacings.len() >= 1 on success");
            Ok(serde_json::Value::Number(serde_json::Number::from(
                i64::from(ts),
            )))
        }
        "last_tick_spacing" => {
            let ts = *tick_spacings
                .last()
                .expect("decode_slipstream_path guarantees tick_spacings.len() >= 1 on success");
            Ok(serde_json::Value::Number(serde_json::Number::from(
                i64::from(ts),
            )))
        }
        "tick_spacing_at_hop" => {
            let idx = extra_arg
                .and_then(serde_json::Value::as_i64)
                .ok_or(FnError::SlipstreamHopIndexMissing)?;
            let resolved = resolve_index(idx, tick_spacings.len())?;
            let ts = tick_spacings[resolved];
            Ok(serde_json::Value::Number(serde_json::Number::from(
                i64::from(ts),
            )))
        }
        other => Err(FnError::UnknownSelect(other.to_owned())),
    }
}

/// Decode a Slipstream packed path into `(tokens, tick_spacings)`.
///
/// Validates: `path.len() == 20 + 23 * N` for some `N >= 1`. Each
/// `tickSpacing` is a 3-byte big-endian `int24`; sign is extended into an
/// `i32` (high bit of byte 0 → `0xFF` padding).
///
/// Returns `(tokens, tick_spacings)` with `tokens.len() == hops + 1` and
/// `tick_spacings.len() == hops`.
fn decode_slipstream_path(
    bytes: &[u8],
) -> Result<(Vec<alloy_primitives::Address>, Vec<i32>), FnError> {
    const ADDR_SIZE: usize = 20;
    const TICK_SIZE: usize = 3;
    const NEXT_OFFSET: usize = ADDR_SIZE + TICK_SIZE; // 23
    const MIN_LEN: usize = NEXT_OFFSET + ADDR_SIZE; // 43 (one full pool)

    if bytes.len() < MIN_LEN {
        return Err(FnError::SlipstreamPathDecode {
            message: format!(
                "path too short: {} bytes (must be >= {} for one hop)",
                bytes.len(),
                MIN_LEN
            ),
        });
    }
    if (bytes.len() - ADDR_SIZE) % NEXT_OFFSET != 0 {
        return Err(FnError::SlipstreamPathDecode {
            message: format!(
                "malformed length {}: must be 20 + 23*N for some N >= 1",
                bytes.len()
            ),
        });
    }

    let pool_count = (bytes.len() - ADDR_SIZE) / NEXT_OFFSET;
    let mut tokens = Vec::with_capacity(pool_count + 1);
    let mut tick_spacings = Vec::with_capacity(pool_count);

    tokens.push(alloy_primitives::Address::from_slice(&bytes[0..ADDR_SIZE]));
    for hop in 0..pool_count {
        let off = ADDR_SIZE + hop * NEXT_OFFSET;
        // Sign-extend int24 (3 bytes big-endian) → i32. High bit of byte[off]
        // is the int24 sign bit; if set, prepend 0xFF to make a 4-byte i32.
        let hi = bytes[off];
        let mid = bytes[off + 1];
        let lo = bytes[off + 2];
        let ts_signed = if hi & 0x80 != 0 {
            i32::from_be_bytes([0xFF, hi, mid, lo])
        } else {
            i32::from_be_bytes([0x00, hi, mid, lo])
        };
        tick_spacings.push(ts_signed);
        tokens.push(alloy_primitives::Address::from_slice(
            &bytes[off + TICK_SIZE..off + TICK_SIZE + ADDR_SIZE],
        ));
    }

    Ok((tokens, tick_spacings))
}

/// `alloy_primitives::Address` → lowercase `0x..` string, validated against
/// the project's [`Address`] hex regex.
fn address_to_json(alloy_addr: alloy_primitives::Address) -> Result<String, FnError> {
    let hex_repr = format!("0x{}", hex::encode(alloy_addr.0));
    let address = Address::from_str(&hex_repr).map_err(|message| FnError::InvalidAddress {
        value: hex_repr.clone(),
        message,
    })?;
    Ok(address.to_string())
}

/// Coerce a JSON value into raw bytes — accepting either the `"0x.."` hex
/// form (default for `DecodedValue::Bytes` via [`super::eval`]) or a JSON
/// array of u8.
fn json_value_to_bytes(value: &serde_json::Value) -> Result<Vec<u8>, FnError> {
    match value {
        serde_json::Value::String(s) => {
            let stripped = s.strip_prefix("0x").unwrap_or(s.as_str());
            hex::decode(stripped).map_err(|e| FnError::BytesShape {
                message: format!("hex decode of {s:?} failed: {e}"),
            })
        }
        serde_json::Value::Array(elements) => {
            let mut out = Vec::with_capacity(elements.len());
            for (i, element) in elements.iter().enumerate() {
                let byte = element.as_u64().ok_or_else(|| FnError::BytesShape {
                    message: format!("array element {i} is not a u64: {element}"),
                })?;
                if byte > 255 {
                    return Err(FnError::BytesShape {
                        message: format!("array element {i} = {byte} > 255"),
                    });
                }
                out.push(byte as u8);
            }
            Ok(out)
        }
        other => Err(FnError::BytesShape {
            message: format!("expected hex string or array, got {other}"),
        }),
    }
}

/// `curve_route_last_token(route: address[11]) -> AddressRef` — Phase 12.3
/// (Curve Router NG output-token resolver).
///
/// Curve Router NG `exchange(...)` encodes a 1-to-5-hop swap path as a fixed-
/// size `address[11]` array zero-padded for unused slots:
///   * `route[0]` — input token
///   * `route[2k]` (k = 1..=5) — intermediate / output token of hop k
///   * `route[2k-1]` (k = 1..=5) — pool address of hop k
///   * unused trailing slots = `address(0)`
///
/// The output token is therefore the *last non-zero address at an even index*.
/// We scan idx 0/2/4/6/8/10 in order and remember the most recent non-zero
/// element; `address(0)` slots are skipped.
///
/// Errors:
///   * [`FnError::TypeMismatch`] — argument is not a JSON array (or any
///     element is not a JSON string).
///   * [`FnError::LengthMismatch`] — array length is not exactly 11.
///   * [`FnError::EmptyRoute`] — every even-index slot is `address(0)`.
///
/// Source: `curvefi/curve-router-ng @ master / contracts/Router.vy::exchange`.
pub fn curve_route_last_token(
    route_value: &serde_json::Value,
) -> Result<serde_json::Value, FnError> {
    let arr = route_value.as_array().ok_or_else(|| FnError::TypeMismatch {
        expected: "array",
        got: route_value.clone(),
    })?;

    if arr.len() != 11 {
        return Err(FnError::LengthMismatch {
            expected: 11,
            got: arr.len(),
        });
    }

    // Curve Router NG `_route` encodes token slots at even indices (0, 2, 4,
    // 6, 8, 10). The last non-zero entry is the output token.
    let mut last_token: Option<&serde_json::Value> = None;
    for i in (0..arr.len()).step_by(2) {
        let entry = &arr[i];
        let addr_str = entry.as_str().ok_or_else(|| FnError::TypeMismatch {
            expected: "address string",
            got: entry.clone(),
        })?;
        if !is_zero_address(addr_str) {
            last_token = Some(entry);
        }
    }

    last_token.cloned().ok_or(FnError::EmptyRoute)
}

/// Case-insensitive comparison against the canonical zero address. Bundles
/// MAY emit either lowercase or EIP-55 mixed-case `0x0...0` — both should
/// resolve to "padded slot".
fn is_zero_address(addr: &str) -> bool {
    addr.eq_ignore_ascii_case("0x0000000000000000000000000000000000000000")
}

/// `select_from_literal_array(array, idx) -> Value` — Phase 12.7 P0-2.
///
/// Pick an element from a bundle-embedded literal array (typically a Curve
/// pool `coins[]`) by a caller-supplied integer index. Used by V1 / V2 / NG
/// `exchange` + `remove_liquidity_one_coin` bundles to resolve `coins[i]` /
/// `coins[j]` instead of hardcoding the first/second token of the pool —
/// the old bundles silently mislabelled inputs and outputs whenever the
/// user passed any `(i, j) != (0, 1)` (P0-2 audit finding).
///
/// `idx` semantics mirror [`select_address`]:
///   * `idx >= 0` — pick `array[idx]`.
///   * `idx <  0` — pick `array[array.len() + idx]` (e.g. `-1` = last).
///
/// `idx_value` may be supplied as a JSON integer, JSON string of a signed
/// decimal integer, or a JSON object wrapper (interpreted via `as_i64`).
/// Curve `exchange` accepts `int128` i/j values which serialize as either
/// `Number` (when the decoder produces small values) or `String` (when the
/// value is large or hex-formatted); both paths are accepted.
///
/// Errors:
///   * [`FnError::TypeMismatch`] — `array_value` is not a JSON array.
///   * [`FnError::TypeMismatch`] — `idx_value` cannot be coerced to `i64`.
///   * [`FnError::IndexOutOfBounds`] — resolved index is outside
///     `0..array.len()`.
pub fn select_from_literal_array(
    array_value: &serde_json::Value,
    idx_value: &serde_json::Value,
) -> Result<serde_json::Value, FnError> {
    let arr = array_value.as_array().ok_or_else(|| FnError::TypeMismatch {
        expected: "array",
        got: array_value.clone(),
    })?;
    let idx = coerce_to_i64(idx_value).ok_or_else(|| FnError::TypeMismatch {
        expected: "integer index",
        got: idx_value.clone(),
    })?;
    let resolved = resolve_index(idx, arr.len())?;
    Ok(arr[resolved].clone())
}

/// Accept `idx_value` as any of: JSON integer, JSON decimal string
/// (`"3"`, `"-1"`), or anything else `as_i64` recognises. Returns `None`
/// when the value cannot be reduced to a signed 64-bit integer.
fn coerce_to_i64(value: &serde_json::Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(n);
    }
    if let Some(u) = value.as_u64() {
        return i64::try_from(u).ok();
    }
    value.as_str().and_then(|s| s.parse::<i64>().ok())
}

fn resolve_index(idx: i64, len: usize) -> Result<usize, FnError> {
    if idx >= 0 {
        let resolved = usize::try_from(idx).map_err(|_| FnError::IndexOutOfBounds { idx, len })?;
        if resolved >= len {
            return Err(FnError::IndexOutOfBounds { idx, len });
        }
        Ok(resolved)
    } else {
        let abs_negative =
            usize::try_from(-idx).map_err(|_| FnError::IndexOutOfBounds { idx, len })?;
        if abs_negative > len {
            return Err(FnError::IndexOutOfBounds { idx, len });
        }
        Ok(len - abs_negative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn addrs() -> serde_json::Value {
        json!([
            "0x1111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222",
            "0x3333333333333333333333333333333333333333",
        ])
    }

    #[test]
    fn select_address_idx_zero_picks_first() {
        let address = select_address(&addrs(), 0).unwrap();
        assert_eq!(
            address.to_string(),
            "0x1111111111111111111111111111111111111111"
        );
    }

    #[test]
    fn select_address_idx_neg_one_picks_last() {
        let address = select_address(&addrs(), -1).unwrap();
        assert_eq!(
            address.to_string(),
            "0x3333333333333333333333333333333333333333"
        );
    }

    #[test]
    fn select_address_idx_neg_two_picks_second_from_last() {
        let address = select_address(&addrs(), -2).unwrap();
        assert_eq!(
            address.to_string(),
            "0x2222222222222222222222222222222222222222"
        );
    }

    #[test]
    fn select_address_idx_oob_pos_errors() {
        let err = select_address(&addrs(), 3).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn select_address_idx_oob_neg_errors() {
        let err = select_address(&addrs(), -4).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn select_address_non_array_errors() {
        let err = select_address(&json!("0x1234"), 0).unwrap_err();
        assert!(matches!(err, FnError::NotArray(_)));
    }

    // ── unfold_v3_path ────────────────────────────────────────────────────

    /// V3 packed path `WETH --0x000bb8--> USDC` (fee 3000 = 0x000bb8).
    /// Total length: 20 + 3 + 20 = 43 bytes — one hop.
    const SINGLE_HOP_PATH_HEX: &str = concat!(
        "0xC02aaa39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "000bb8",
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
    );

    /// Two-hop path: `WETH --3000--> USDC --500--> USDT`.
    /// Length = 20 + 3 + 20 + 3 + 20 = 66 bytes.
    const TWO_HOP_PATH_HEX: &str = concat!(
        "0xC02aaa39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "000bb8",
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        "0001f4",
        "dAC17F958D2ee523a2206206994597C13D831ec7",
    );

    #[test]
    fn unfold_v3_path_first_token_single_hop() {
        let value = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "first_token").unwrap();
        // WETH (lowercased — policy_engine::action::Address normalises).
        assert_eq!(
            value.as_str().unwrap(),
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        );
    }

    #[test]
    fn unfold_v3_path_last_token_single_hop() {
        let value = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "last_token").unwrap();
        // USDC
        assert_eq!(
            value.as_str().unwrap(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
    }

    #[test]
    fn unfold_v3_path_multi_hop_endpoints() {
        let first = unfold_v3_path(&json!(TWO_HOP_PATH_HEX), "first_token").unwrap();
        let last = unfold_v3_path(&json!(TWO_HOP_PATH_HEX), "last_token").unwrap();
        // First = WETH, last = USDT (third token in the chain).
        assert_eq!(
            first.as_str().unwrap(),
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        );
        assert_eq!(
            last.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn unfold_v3_path_accepts_array_of_u8() {
        // Same single-hop path, but supplied as a JSON array of octets.
        let raw = hex::decode(SINGLE_HOP_PATH_HEX.strip_prefix("0x").unwrap()).unwrap();
        let array_json =
            serde_json::Value::Array(raw.iter().map(|b| json!(*b)).collect());
        let first = unfold_v3_path(&array_json, "first_token").unwrap();
        assert_eq!(
            first.as_str().unwrap(),
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        );
    }

    // ── unfold_v3_path: fee modes (Phase 7B / T-B3) ─────────────────────

    /// Two-hop path `USDT --0x0001f4(500)--> USDC --0x000bb8(3000)--> WETH`.
    /// 66 bytes (20 + 3 + 20 + 3 + 20).
    const FEE_TWO_HOP_PATH_HEX: &str = concat!(
        "0xdAC17F958D2ee523a2206206994597C13D831ec7",
        "0001f4",
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        "000bb8",
        "C02aaa39b223FE8D0A0e5C4F27eAD9083C756Cc2",
    );

    #[test]
    fn unfold_v3_path_first_fee_returns_500_for_two_hop() {
        let value = unfold_v3_path(&json!(FEE_TWO_HOP_PATH_HEX), "first_fee").unwrap();
        assert_eq!(value, json!(500));
    }

    #[test]
    fn unfold_v3_path_last_fee_returns_3000() {
        let value = unfold_v3_path(&json!(FEE_TWO_HOP_PATH_HEX), "last_fee").unwrap();
        assert_eq!(value, json!(3000));
    }

    #[test]
    fn unfold_v3_path_single_hop_first_fee_equals_last_fee() {
        let first = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "first_fee").unwrap();
        let last = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "last_fee").unwrap();
        // Both endpoints coincide on a one-hop path — the fee is 3000.
        assert_eq!(first, json!(3000));
        assert_eq!(last, json!(3000));
    }

    #[test]
    fn unfold_v3_path_too_short_errors() {
        // 20 + 22 = 42 bytes — neither single token nor a full hop.
        let bytes_hex = format!("0x{}", "11".repeat(42));
        let err = unfold_v3_path(&json!(bytes_hex), "first_token").unwrap_err();
        assert!(matches!(err, FnError::PathDecode { .. }));
    }

    #[test]
    fn unfold_v3_path_unknown_select_errors() {
        let err = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "middle_token").unwrap_err();
        assert!(matches!(err, FnError::UnknownSelect(_)));
    }

    #[test]
    fn unfold_v3_path_non_bytes_errors() {
        let err = unfold_v3_path(&json!(42), "first_token").unwrap_err();
        assert!(matches!(err, FnError::BytesShape { .. }));
    }

    // ── unfold_slipstream_path (Phase 8 — Aerodrome CL) ──────────────────

    /// Single-hop Slipstream path with positive `tickSpacing=100` (0x000064):
    /// `0xaaa…01 --100--> 0xbbb…02`. Length = 20 + 3 + 20 = 43 bytes.
    const SLIP_SINGLE_HOP_HEX: &str = concat!(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
        "000064",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
    );

    #[test]
    fn unfold_slipstream_path_single_hop_first_last_token() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let first = unfold_slipstream_path(&v, "first_token", None).unwrap();
        let last = unfold_slipstream_path(&v, "last_token", None).unwrap();
        assert_eq!(
            first.as_str().unwrap(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01"
        );
        assert_eq!(
            last.as_str().unwrap(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02"
        );
    }

    #[test]
    fn unfold_slipstream_path_first_tick_spacing_positive() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_negative_tick_spacing_sign_extension() {
        // tickSpacing = -1 = 0xFFFFFF (int24 — all bits set, sign bit set)
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "ffffff",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), -1);
    }

    #[test]
    fn unfold_slipstream_path_max_positive_tick_spacing() {
        // tickSpacing = 0x7FFFFF = 8_388_607 (int24 max positive)
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "7fffff",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), 8_388_607);
    }

    #[test]
    fn unfold_slipstream_path_min_negative_tick_spacing() {
        // tickSpacing = 0x800000 = -8_388_608 (int24 min negative)
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "800000",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), -8_388_608);
    }

    #[test]
    fn unfold_slipstream_path_two_hop_tick_spacing_at_hop() {
        // tokenA + ts=50 (0x000032) + tokenB + ts=100 (0x000064) + tokenC
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "000032",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
            "000064",
            "cccccccccccccccccccccccccccccccccccccc03",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let hop0 =
            unfold_slipstream_path(&v, "tick_spacing_at_hop", Some(&json!(0))).unwrap();
        let hop1 =
            unfold_slipstream_path(&v, "tick_spacing_at_hop", Some(&json!(1))).unwrap();
        let hop_last =
            unfold_slipstream_path(&v, "tick_spacing_at_hop", Some(&json!(-1))).unwrap();
        assert_eq!(hop0.as_i64().unwrap(), 50);
        assert_eq!(hop1.as_i64().unwrap(), 100);
        assert_eq!(hop_last.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_two_hop_last_tick_spacing() {
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "000032",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
            "000064",
            "cccccccccccccccccccccccccccccccccccccc03",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let first = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        let last = unfold_slipstream_path(&v, "last_tick_spacing", None).unwrap();
        assert_eq!(first.as_i64().unwrap(), 50);
        assert_eq!(last.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_malformed_too_short() {
        // length 30 — too short for one full pool (need >= 43)
        let bytes: Vec<u8> = vec![0xAA; 30];
        let v = serde_json::Value::Array(bytes.into_iter().map(|b| json!(b)).collect());
        let err = unfold_slipstream_path(&v, "first_token", None).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamPathDecode { .. }));
    }

    #[test]
    fn unfold_slipstream_path_malformed_misaligned() {
        // length 50 — not 20 + 23*N for any N (20+23=43, 20+46=66)
        let bytes: Vec<u8> = vec![0xAA; 50];
        let v = serde_json::Value::Array(bytes.into_iter().map(|b| json!(b)).collect());
        let err = unfold_slipstream_path(&v, "first_token", None).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamPathDecode { .. }));
    }

    #[test]
    fn unfold_slipstream_path_unknown_select() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let err = unfold_slipstream_path(&v, "invalid_select", None).unwrap_err();
        assert!(matches!(err, FnError::UnknownSelect(_)));
    }

    #[test]
    fn unfold_slipstream_path_accepts_array_of_u8() {
        let raw = hex::decode(SLIP_SINGLE_HOP_HEX.strip_prefix("0x").unwrap()).unwrap();
        let v = serde_json::Value::Array(raw.iter().map(|b| json!(*b)).collect());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_tick_spacing_at_hop_without_arg_errors() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let err = unfold_slipstream_path(&v, "tick_spacing_at_hop", None).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamHopIndexMissing));
    }
}

#[cfg(test)]
mod tests_curve_route_last_token {
    use super::*;
    use serde_json::json;

    /// 1-hop route: `_route = [USDC, 3pool, USDT, 0×8]`. Output token = idx 2
    /// (USDT). idx 3..=10 are zero-padded.
    #[test]
    fn one_hop_route_returns_idx_2() {
        let route = json!([
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // [0] USDC
            "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7", // [1] 3pool
            "0xdac17f958d2ee523a2206206994597c13d831ec7", // [2] USDT
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    /// 5-hop route: every even idx (0/2/4/6/8/10) has a token, every odd idx
    /// has a pool. Output token = idx 10.
    #[test]
    fn five_hop_route_returns_idx_10() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1", // [0] input
            "0x1111111111111111111111111111111111111111", // [1] pool1
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2", // [2] mid1
            "0x2222222222222222222222222222222222222222", // [3] pool2
            "0xcccccccccccccccccccccccccccccccccccccccc", // [4] mid2
            "0x3333333333333333333333333333333333333333", // [5] pool3
            "0xdddddddddddddddddddddddddddddddddddddddd", // [6] mid3
            "0x4444444444444444444444444444444444444444", // [7] pool4
            "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", // [8] mid4
            "0x5555555555555555555555555555555555555555", // [9] pool5
            "0xfffffffffffffffffffffffffffffffffffffff5", // [10] output
        ]);
        let result = curve_route_last_token(&route).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "0xfffffffffffffffffffffffffffffffffffffff5"
        );
    }

    /// All-zero route → EmptyRoute. This shouldn't happen in real calldata
    /// (Router NG `exchange` requires at least one hop) but the resolver
    /// must fail closed.
    #[test]
    fn empty_route_returns_error() {
        let route = json!([
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let err = curve_route_last_token(&route).unwrap_err();
        assert!(matches!(err, FnError::EmptyRoute));
    }

    /// Length validation — Curve Router NG always passes exactly 11 slots.
    /// Any other length is a calldata corruption (or wrong decoder).
    #[test]
    fn wrong_length_returns_error() {
        let route = json!([
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let err = curve_route_last_token(&route).unwrap_err();
        assert!(matches!(
            err,
            FnError::LengthMismatch {
                expected: 11,
                got: 2
            }
        ));
    }

    /// Argument-type validation — non-array values surface as TypeMismatch
    /// (vs panic-on-cast).
    #[test]
    fn non_array_returns_error() {
        let err = curve_route_last_token(&json!("0xdead")).unwrap_err();
        assert!(matches!(err, FnError::TypeMismatch { expected: "array", .. }));
    }
}

#[cfg(test)]
mod tests_select_from_literal_array {
    use super::*;
    use serde_json::json;

    fn coins() -> serde_json::Value {
        // Curve 3pool coins: DAI / USDC / USDT.
        json!([
            "0x6b175474e89094c44da98b954eedeac495271d0f",
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0xdac17f958d2ee523a2206206994597c13d831ec7",
        ])
    }

    #[test]
    fn idx_zero_returns_first_coin_dai() {
        let value = select_from_literal_array(&coins(), &json!(0)).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0x6b175474e89094c44da98b954eedeac495271d0f"
        );
    }

    #[test]
    fn idx_two_returns_usdt() {
        // P0-2 anchor — the previous bundles hardcoded coins[0] / coins[1],
        // so a tx with `i = 2` would mislabel the input token as DAI.
        let value = select_from_literal_array(&coins(), &json!(2)).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn idx_neg_one_returns_last() {
        let value = select_from_literal_array(&coins(), &json!(-1)).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn idx_decimal_string_is_accepted() {
        // Curve int128 i/j values can serialize through the decoder as
        // strings — make sure both paths produce the same lookup.
        let value = select_from_literal_array(&coins(), &json!("1")).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
    }

    #[test]
    fn idx_negative_decimal_string_is_accepted() {
        let value = select_from_literal_array(&coins(), &json!("-1")).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn out_of_bounds_positive_errors() {
        let err = select_from_literal_array(&coins(), &json!(5)).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn out_of_bounds_negative_errors() {
        let err = select_from_literal_array(&coins(), &json!(-4)).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn non_array_input_errors() {
        let err =
            select_from_literal_array(&json!("0xdeadbeef"), &json!(0)).unwrap_err();
        assert!(matches!(err, FnError::TypeMismatch { expected: "array", .. }));
    }

    #[test]
    fn non_integer_index_errors() {
        let err = select_from_literal_array(&coins(), &json!("not-a-number")).unwrap_err();
        assert!(matches!(
            err,
            FnError::TypeMismatch {
                expected: "integer index",
                ..
            }
        ));
    }
}
