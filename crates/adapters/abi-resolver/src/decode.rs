//! Standard-ABI dynamic decoder.
//!
//! Given an `alloy_json_abi::Function` (from Sourcify) or a parsed signature
//! string (from openchain), decode the standard-ABI portion of a transaction's
//! calldata into named argument values.
//!
//! Non-standard payloads (V3 packed path, Universal Router commands, etc.)
//! intentionally stay opaque here — they surface as raw `bytes` values, and
//! the first-party adapters in `crates/adapters/*` handle them precisely.

use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::{Function, Param};

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
}
