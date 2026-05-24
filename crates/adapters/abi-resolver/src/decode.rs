//! Standard-ABI dynamic decoder.
//!
//! Given an `alloy_json_abi::Function` (from Sourcify) or a parsed signature
//! string (from openchain), decode the standard-ABI portion of a transaction's
//! calldata into named argument values.
//!
//! Non-standard payloads (V3 packed path, Universal Router commands, etc.)
//! intentionally stay opaque here — they surface as raw `bytes` values, and
//! the first-party adapters in `crates/adapters/*` handle them precisely.

use alloy_dyn_abi::{DynSolType, DynSolValue, JsonAbiExt};
use alloy_json_abi::{Function, Param};
use std::str::FromStr;

/// One decoded argument paired with the name we surface to callers.
///
/// `name` is the metadata's parameter name when available, otherwise a
/// synthetic `arg{index}` so output remains stable when only openchain has the
/// signature (no parameter names there).
///
/// `components` mirrors `Param.components` for the source parameter. It's how
/// `format_value_named` recovers tuple field names — alloy's `DynSolValue`
/// stores only positional values, so a sibling descriptor is required.
#[derive(Debug, Clone)]
pub struct DecodedArg {
    pub name: String,
    pub sol_type: String,
    pub value: DynSolValue,
    pub components: Vec<Param>,
}

/// Result of a successful decode.
#[derive(Debug, Clone)]
pub struct DecodedCall {
    pub function_name: String,
    pub signature: String,
    pub args: Vec<DecodedArg>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("calldata too short: expected at least 4 bytes for selector, got {0}")]
    TooShort(usize),
    #[error("calldata selector {got} does not match function selector {expected}")]
    SelectorMismatch { got: String, expected: String },
    #[error("ABI decode failed: {0}")]
    AbiDecode(String),
    #[error("invalid signature {0}: {1}")]
    BadSignature(String, String),
    #[error("argument count mismatch: expected {expected}, got {got}")]
    ArgCountMismatch { expected: usize, got: usize },
    /// Phase B / F4 — strict-mode length validation rejected calldata whose
    /// args portion does not match the ABI-declared static minimum exactly.
    /// Surfaces with the message `length mismatch` so callers can pattern-match.
    #[error("decode_failed: length mismatch — expected args of {expected_bytes} bytes for static signature, got {got_bytes}")]
    LengthMismatch {
        expected_bytes: usize,
        got_bytes: usize,
    },
}

/// Decode calldata using a fully-parsed `Function` (Sourcify path).
///
/// The function's selector is verified against the first 4 bytes of `calldata`
/// before the body is decoded.
///
/// # Errors
///
/// Returns `DecodeError` when the selector mismatches, calldata is short, or
/// the body fails ABI decoding.
pub fn decode_with_function(
    function: &Function,
    calldata: &[u8],
) -> Result<DecodedCall, DecodeError> {
    if calldata.len() < 4 {
        return Err(DecodeError::TooShort(calldata.len()));
    }
    let got_selector = [calldata[0], calldata[1], calldata[2], calldata[3]];
    let expected_selector = function.selector().0;
    if got_selector != expected_selector {
        return Err(DecodeError::SelectorMismatch {
            got: format!("0x{}", hex::encode(got_selector)),
            expected: format!("0x{}", hex::encode(expected_selector)),
        });
    }

    // `validate=false` so dapp-appended trailing metadata bytes (Uniswap/1inch
    // frontend attribution tags, etc.) don't trip alloy's strict reserialization
    // check. The EVM contract itself ignores trailing bytes after the selector's
    // declared inputs; we follow the same convention. ABI argument extraction
    // remains exact — strict mode only gates trailing-byte reject.
    let values = function
        .abi_decode_input(&calldata[4..], false)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;

    if values.len() != function.inputs.len() {
        return Err(DecodeError::ArgCountMismatch {
            expected: function.inputs.len(),
            got: values.len(),
        });
    }

    let args = function
        .inputs
        .iter()
        .enumerate()
        .zip(values)
        .map(|((idx, param), value)| {
            let name = if param.name.is_empty() {
                format!("arg{idx}")
            } else {
                param.name.clone()
            };
            DecodedArg {
                name,
                sol_type: param.ty.clone(),
                value,
                components: param.components.clone(),
            }
        })
        .collect();

    Ok(DecodedCall {
        function_name: function.name.clone(),
        signature: function.signature(),
        args,
    })
}

/// Decode calldata given only a canonical signature string (openchain path).
///
/// Argument names default to `arg0, arg1, ...` because openchain doesn't carry
/// parameter names.
///
/// # Errors
///
/// Returns `DecodeError::BadSignature` when the signature is unparseable, or
/// any of the errors documented on `decode_with_function`.
pub fn decode_with_signature(signature: &str, calldata: &[u8]) -> Result<DecodedCall, DecodeError> {
    let function = Function::parse(signature)
        .map_err(|e| DecodeError::BadSignature(signature.into(), e.to_string()))?;
    decode_with_function(&function, calldata)
}

/// Compute the **exact** static encoding byte length required by a function's
/// inputs (Phase B / F4 helper).
///
/// Returns `Some(n)` only when every input parses as a [`DynSolType`] **and**
/// every input is statically sized (no `bytes`, `string`, `T[]`, or
/// tuple-with-dynamic). In that case `n = 32 * sum(min_words(param))`. Returns
/// `None` for unparseable types or when any input is dynamic — callers should
/// then skip the strict length check and fall back to permissive decoding.
///
/// Used by [`validate_calldata_length`] to catch the F4 misdecode class:
/// non-standard-padded calldata fed to a fixed-arg signature. Pure helper —
/// does not look at calldata.
#[must_use]
pub fn expected_static_args_len(function: &Function) -> Option<usize> {
    let mut words: usize = 0;
    for param in &function.inputs {
        let ty = DynSolType::from_str(&param.ty).ok()?;
        if is_dynamic_type(&ty) {
            return None;
        }
        words = words.checked_add(ty.minimum_words())?;
    }
    words.checked_mul(32)
}

/// Recursive dynamism check.
///
/// `DynSolType::Bytes` / `::String` / `::Array(_)` are top-level dynamic.
/// Tuples and fixed-arrays are dynamic iff any element is dynamic. Custom
/// structs (eip712) recurse into their tuple as well.
fn is_dynamic_type(ty: &DynSolType) -> bool {
    match ty {
        DynSolType::Bytes | DynSolType::String | DynSolType::Array(_) => true,
        DynSolType::FixedArray(inner, _) => is_dynamic_type(inner),
        DynSolType::Tuple(elements) => elements.iter().any(is_dynamic_type),
        _ => false,
    }
}

/// Phase B / F4 — strict-mode length validation.
///
/// When the function has **only static inputs** (no `bytes` / `string` / arrays
/// / dynamic tuples), reject calldata whose args portion does not match the
/// ABI-declared encoding **exactly**. Returns `Ok(())` for functions with any
/// dynamic input (the offset-based ABI tolerates variable lengths so we cannot
/// length-check them statically). Returns [`DecodeError::TooShort`] when the
/// selector itself isn't present.
///
/// # Why exact (not min) for static functions
///
/// Non-multiple-of-32 calldata is the F4 misdecode signature (e.g. Curve
/// VotingEscrow `create_lock` `tx 0x0d1c1872…` — 4 sel + 65 args bytes for a
/// 2-uint256 ABI, alloy `validate=false` silently padded and shifted, producing
/// `unlockTime=27273=1970` instead of `1787184000=2026-08`). For fully-static
/// signatures the canonical encoding length is `32 * n_words`; anything else
/// is either truncated or has a trailer that risks shifting. Strict mode is
/// opt-in via [`decode_with_function_strict`] (or [`decode_with_signature_strict`])
/// because the permissive `decode_with_function` path must keep tolerating the
/// Aerodrome wallet-suffix pattern (32-byte uint256 + 43-byte trailer = 75
/// bytes, non-multiple-of-32, where the trailer is a wallet attribution tag
/// that the EVM contract ignores after consuming the declared inputs).
///
/// # Errors
///
/// * [`DecodeError::TooShort`] — `calldata.len() < 4`.
/// * [`DecodeError::LengthMismatch`] — function has only static inputs and the
///   args portion `(calldata.len() - 4)` differs from `expected_static_args_len`.
pub fn validate_calldata_length(function: &Function, calldata: &[u8]) -> Result<(), DecodeError> {
    if calldata.len() < 4 {
        return Err(DecodeError::TooShort(calldata.len()));
    }
    let Some(expected) = expected_static_args_len(function) else {
        // Function has dynamic inputs — fall back to permissive decoding.
        return Ok(());
    };
    let got = calldata.len() - 4;
    if got != expected {
        return Err(DecodeError::LengthMismatch {
            expected_bytes: expected,
            got_bytes: got,
        });
    }
    Ok(())
}

/// Strict-mode variant of [`decode_with_function`] — applies
/// [`validate_calldata_length`] before decoding. Fails fast on
/// non-canonically-encoded calldata for fully-static signatures, which catches
/// the F4 misdecode class without affecting dynamic-arg ABIs.
///
/// # Errors
///
/// All errors from [`decode_with_function`] plus
/// [`DecodeError::LengthMismatch`] when strict length validation rejects the
/// calldata.
pub fn decode_with_function_strict(
    function: &Function,
    calldata: &[u8],
) -> Result<DecodedCall, DecodeError> {
    validate_calldata_length(function, calldata)?;
    decode_with_function(function, calldata)
}

/// Strict-mode variant of [`decode_with_signature`] — applies
/// [`validate_calldata_length`] before decoding.
///
/// # Errors
///
/// All errors from [`decode_with_signature`] plus
/// [`DecodeError::LengthMismatch`].
pub fn decode_with_signature_strict(
    signature: &str,
    calldata: &[u8],
) -> Result<DecodedCall, DecodeError> {
    let function = Function::parse(signature)
        .map_err(|e| DecodeError::BadSignature(signature.into(), e.to_string()))?;
    decode_with_function_strict(&function, calldata)
}

/// Render a decoded value as a human-readable string. Best-effort: addresses
/// keep their checksum, integers go decimal, bytes stay hex.
///
/// This variant has no access to ABI parameter names, so tuples render as
/// `(v0, v1, v2)`. Use `format_value_named` to pick up tuple field names
/// when you have the matching `Param.components`.
#[must_use]
pub fn format_value(value: &DynSolValue) -> String {
    format_value_named(value, &[])
}

/// Render a decoded value with tuple field names attached, recursively.
///
/// `components` is the `Param.components` slice for the value being formatted
/// (empty for primitives, populated for tuples / arrays of tuples). When a
/// tuple field has a non-empty `name`, the rendering becomes
/// `(field: value, …)`; otherwise it falls back to the unnamed form.
///
/// Arrays propagate the element descriptor: an `Order[]` and an `Order` share
/// the same component layout.
#[must_use]
pub fn format_value_named(value: &DynSolValue, components: &[Param]) -> String {
    match value {
        DynSolValue::Address(a) => format!("0x{}", hex::encode(a.0)),
        DynSolValue::Bool(b) => b.to_string(),
        DynSolValue::Bytes(b) => format!("0x{}", hex::encode(b)),
        DynSolValue::FixedBytes(word, size) => {
            format!("0x{}", hex::encode(&word.0[..*size]))
        }
        DynSolValue::Int(i, _) => i.to_string(),
        DynSolValue::Uint(u, _) => u.to_string(),
        DynSolValue::String(s) => format!("\"{s}\""),
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => {
            let inner = items
                .iter()
                .map(|v| format_value_named(v, components))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{inner}]")
        }
        DynSolValue::Tuple(items) => {
            let inner = items
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let child = components.get(i);
                    let child_components = child.map(|c| c.components.as_slice()).unwrap_or(&[]);
                    let formatted = format_value_named(v, child_components);
                    match child {
                        Some(c) if !c.name.is_empty() => format!("{}: {}", c.name, formatted),
                        _ => formatted,
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({inner})")
        }
        DynSolValue::Function(_) => "<function>".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{hex as alloy_hex, U256};

    fn encode_approve_call(spender: [u8; 20], amount: u128) -> Vec<u8> {
        // selector for `approve(address,uint256)` = 0x095ea7b3
        let mut data = vec![0x09, 0x5e, 0xa7, 0xb3];
        let mut spender_word = [0u8; 32];
        spender_word[12..].copy_from_slice(&spender);
        data.extend_from_slice(&spender_word);
        let amount_bytes: [u8; 32] = U256::from(amount).to_be_bytes();
        data.extend_from_slice(&amount_bytes);
        data
    }

    #[test]
    fn decode_with_signature_extracts_named_args() {
        let calldata = encode_approve_call([0x11; 20], 12345);
        let decoded = decode_with_signature("approve(address,uint256)", &calldata).unwrap();

        assert_eq!(decoded.function_name, "approve");
        assert_eq!(decoded.signature, "approve(address,uint256)");
        assert_eq!(decoded.args.len(), 2);
        assert_eq!(decoded.args[0].name, "arg0"); // signature has no param names
        assert_eq!(decoded.args[1].name, "arg1");
        assert_eq!(decoded.args[0].sol_type, "address");
        assert_eq!(decoded.args[1].sol_type, "uint256");
    }

    #[test]
    fn decode_with_function_uses_param_names_when_present() {
        let calldata = encode_approve_call([0x11; 20], 100);
        let abi_json = serde_json::json!({
            "name": "approve",
            "type": "function",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount",  "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        });
        let function: Function = serde_json::from_value(abi_json).unwrap();
        let decoded = decode_with_function(&function, &calldata).unwrap();

        assert_eq!(decoded.args[0].name, "spender");
        assert_eq!(decoded.args[1].name, "amount");
        assert!(matches!(decoded.args[0].value, DynSolValue::Address(_)));
        assert!(matches!(decoded.args[1].value, DynSolValue::Uint(_, 256)));
    }

    #[test]
    fn rejects_short_calldata() {
        let result = decode_with_signature("approve(address,uint256)", &[0x09, 0x5e]);
        assert!(matches!(result, Err(DecodeError::TooShort(2))));
    }

    #[test]
    fn rejects_selector_mismatch() {
        let mut calldata = encode_approve_call([0x11; 20], 1);
        calldata[0] = 0xff;
        let result = decode_with_signature("approve(address,uint256)", &calldata);
        assert!(matches!(result, Err(DecodeError::SelectorMismatch { .. })));
    }

    #[test]
    fn rejects_unparseable_signature() {
        let result = decode_with_signature("not a sig", &[0; 4]);
        assert!(matches!(result, Err(DecodeError::BadSignature(_, _))));
    }

    #[test]
    fn format_value_renders_basic_types() {
        let addr = DynSolValue::Address(alloy_primitives::Address::from([0x11; 20]));
        assert_eq!(
            format_value(&addr),
            "0x1111111111111111111111111111111111111111"
        );
        assert_eq!(
            format_value(&DynSolValue::Uint(U256::from(123u64), 256)),
            "123"
        );
        assert_eq!(format_value(&DynSolValue::Bool(true)), "true");
    }

    // Reference encoded approve calldata (matches the exactly-100 amount, addr 0x11..).
    // Keeps a regression anchor in case alloy changes encoding semantics.
    #[test]
    fn reference_encoding_decodes_to_expected_amount() {
        let calldata = encode_approve_call([0x11; 20], 100);
        let decoded = decode_with_signature("approve(address,uint256)", &calldata).unwrap();
        assert_eq!(format_value(&decoded.args[1].value), "100");
        let _ = alloy_hex::encode(&calldata); // smoke test that alloy is wired in
    }

    // ── Phase B / F4 — strict-mode length validation tests ───────────────────

    #[test]
    fn expected_static_args_len_two_uint256() {
        // approve(address,uint256) = 2 static args × 32 = 64 bytes.
        let function = Function::parse("approve(address,uint256)").unwrap();
        assert_eq!(expected_static_args_len(&function), Some(64));
    }

    #[test]
    fn expected_static_args_len_create_lock() {
        // veCRV create_lock(uint256,uint256) — the F4 root-cause signature.
        let function = Function::parse("create_lock(uint256,uint256)").unwrap();
        assert_eq!(expected_static_args_len(&function), Some(64));
    }

    #[test]
    fn expected_static_args_len_dynamic_returns_none() {
        // Any dynamic input poisons static length checking.
        let function = Function::parse("multicall(bytes[])").unwrap();
        assert_eq!(expected_static_args_len(&function), None);
        let function = Function::parse("foo(string)").unwrap();
        assert_eq!(expected_static_args_len(&function), None);
        let function = Function::parse("foo(uint256[])").unwrap();
        assert_eq!(expected_static_args_len(&function), None);
    }

    #[test]
    fn validate_calldata_length_accepts_canonical() {
        let function = Function::parse("approve(address,uint256)").unwrap();
        let calldata = encode_approve_call([0x11; 20], 100);
        assert!(validate_calldata_length(&function, &calldata).is_ok());
    }

    #[test]
    fn validate_calldata_length_rejects_truncated_static() {
        // 64 - 1 = 63 bytes args, expected 64. F4-style truncation.
        let function = Function::parse("approve(address,uint256)").unwrap();
        let canonical = encode_approve_call([0x11; 20], 100);
        let truncated: Vec<u8> = canonical[..canonical.len() - 1].to_vec();
        let err = validate_calldata_length(&function, &truncated).unwrap_err();
        assert!(
            matches!(
                err,
                DecodeError::LengthMismatch {
                    expected_bytes: 64,
                    got_bytes: 63
                }
            ),
            "expected LengthMismatch(64,63), got: {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("length mismatch"),
            "error string must contain 'length mismatch' for harness pattern-matching, got: {msg}"
        );
    }

    #[test]
    fn validate_calldata_length_rejects_non_multiple_of_32_trailer() {
        // 64 + 1 = 65 bytes args. F4 root cause (non-mult-of-32 trailer).
        let function = Function::parse("create_lock(uint256,uint256)").unwrap();
        let mut calldata = vec![0x65, 0xfc, 0x38, 0x73]; // create_lock selector
                                                         // value = 1250e18 as 32 bytes BE
        let value: [u8; 32] = U256::from(1250_u128 * 10_u128.pow(18)).to_be_bytes();
        calldata.extend_from_slice(&value);
        // unlockTime = 1787184000 as 32 bytes BE
        let unlock: [u8; 32] = U256::from(1_787_184_000_u64).to_be_bytes();
        calldata.extend_from_slice(&unlock);
        // The F4 non-standard trailer — one extra byte (mimics
        // tx 0x0d1c1872... padding pattern that misled alloy).
        calldata.push(0xff);
        let err = validate_calldata_length(&function, &calldata).unwrap_err();
        assert!(
            matches!(
                err,
                DecodeError::LengthMismatch {
                    expected_bytes: 64,
                    got_bytes: 65
                }
            ),
            "expected LengthMismatch(64,65), got: {err:?}"
        );
    }

    #[test]
    fn validate_calldata_length_skips_dynamic_signatures() {
        // multicall(bytes[]) has dynamic inputs — strict check must NOT
        // reject calldata regardless of length (the offset-based ABI
        // allows arbitrary payload sizes).
        let function = Function::parse("multicall(bytes[])").unwrap();
        // Anything ≥ 4 bytes should be Ok.
        let calldata = vec![0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03];
        assert!(validate_calldata_length(&function, &calldata).is_ok());
    }

    #[test]
    fn validate_calldata_length_rejects_short_for_dynamic_too() {
        // Selector itself missing — always rejected regardless of dynamism.
        let function = Function::parse("multicall(bytes[])").unwrap();
        let err = validate_calldata_length(&function, &[0x01]).unwrap_err();
        assert!(matches!(err, DecodeError::TooShort(1)));
    }

    #[test]
    fn decode_with_function_strict_rejects_f4_pattern() {
        // End-to-end: strict decoder rejects what permissive decoder
        // would silently misdecode.
        let function = Function::parse("create_lock(uint256,uint256)").unwrap();
        let mut calldata = vec![0x65, 0xfc, 0x38, 0x73];
        let value: [u8; 32] = U256::from(1250_u128 * 10_u128.pow(18)).to_be_bytes();
        calldata.extend_from_slice(&value);
        let unlock: [u8; 32] = U256::from(1_787_184_000_u64).to_be_bytes();
        calldata.extend_from_slice(&unlock);
        calldata.push(0xff);
        let err = decode_with_function_strict(&function, &calldata).unwrap_err();
        assert!(
            matches!(err, DecodeError::LengthMismatch { .. }),
            "strict decoder must fail-closed on non-canonical static-arg padding, got: {err:?}"
        );
    }

    #[test]
    fn decode_with_function_strict_accepts_canonical() {
        // Strict mode is a no-op for well-formed calldata.
        let function = Function::parse("approve(address,uint256)").unwrap();
        let calldata = encode_approve_call([0x11; 20], 100);
        let decoded = decode_with_function_strict(&function, &calldata).unwrap();
        assert_eq!(decoded.args.len(), 2);
        assert_eq!(format_value(&decoded.args[1].value), "100");
    }

    #[test]
    fn decode_with_function_strict_accepts_dynamic_with_trailer() {
        // For dynamic-input functions the strict check is skipped — trailers
        // are tolerated exactly like permissive mode.
        // multicall(bytes[]) with one empty bytes element + a stray trailing
        // byte. The dynamic decoder consumes the head/tail offsets; trailers
        // beyond the declared payload are tolerated by alloy with validate=false.
        let function = Function::parse("multicall(bytes[])").unwrap();
        let mut calldata = vec![0xac, 0x96, 0x50, 0xd8]; // multicall(bytes[]) selector
                                                         // Offset to the bytes[] head (32 bytes from start of args).
        let offset: [u8; 32] = U256::from(32_u64).to_be_bytes();
        calldata.extend_from_slice(&offset);
        // Length = 0.
        let zero: [u8; 32] = U256::from(0_u64).to_be_bytes();
        calldata.extend_from_slice(&zero);
        // Trailer (the wallet-suffix analogue for dynamic functions).
        calldata.push(0xff);
        // Strict mode must not reject — dynamism poisons the static check.
        assert!(decode_with_function_strict(&function, &calldata).is_ok());
    }
}
