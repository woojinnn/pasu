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
}
