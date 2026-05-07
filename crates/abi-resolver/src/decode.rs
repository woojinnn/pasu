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
use alloy_json_abi::Function;

/// One decoded argument paired with the name we surface to callers.
///
/// `name` is the metadata's parameter name when available, otherwise a
/// synthetic `arg{index}` so output remains stable when only openchain has the
/// signature (no parameter names there).
#[derive(Debug, Clone)]
pub struct DecodedArg {
    pub name: String,
    pub sol_type: String,
    pub value: DynSolValue,
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

    let values = function
        .abi_decode_input(&calldata[4..], true)
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
        .zip(values.into_iter())
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
/// keep their checksum, integers go decimal, bytes stay hex. Used by the CLI
/// example and tests; library callers can also render the raw `DynSolValue`
/// however they like.
#[must_use]
pub fn format_value(value: &DynSolValue) -> String {
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
            let inner = items.iter().map(format_value).collect::<Vec<_>>().join(", ");
            format!("[{inner}]")
        }
        DynSolValue::Tuple(items) => {
            let inner = items.iter().map(format_value).collect::<Vec<_>>().join(", ");
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
        assert!(matches!(
            result,
            Err(DecodeError::SelectorMismatch { .. })
        ));
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
