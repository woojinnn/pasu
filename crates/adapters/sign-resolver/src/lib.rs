//! `sign-resolver`
//!
//! Decodes EVM sign RPC requests into a structured [`SignRequest`].
//!
//! Supported methods:
//! - `eth_signTypedData_v4`  — EIP-712 typed data
//! - `personal_sign`         — raw message (hex-encoded)
//! - `eth_sign`              — raw 32-byte hash (deprecated)
//! - `eth_signTransaction`   — unsigned tx object
//! - `eth_sendUserOperation` — ERC-4337 UserOperation
//! - `wallet_grantPermissions` — ERC-7715 permission request
//!
//! Non-sign methods (e.g. `eth_sendTransaction`) return
//! [`SignResolveError::UnsupportedMethod`].

pub mod error;
pub mod method;
pub mod payload;

pub use error::SignResolveError;
pub use method::SignMethod;
pub use payload::SignPayload;

use serde_json::Value;

/// Fully decoded sign request.
#[derive(Debug, Clone)]
pub struct SignRequest {
    /// RPC method that produced this request.
    pub method: SignMethod,
    /// Normalized (lowercase) signer address.
    pub signer: String,
    /// Effective chain id — domain.chainId when available, otherwise the
    /// wallet's current chain id passed by the caller.
    pub chain_id: u64,
    /// Decoded payload, variant-per-method.
    pub payload: SignPayload,
}

/// Decode a raw RPC sign request into a [`SignRequest`].
///
/// # Parameters
/// - `method`   — RPC method string (e.g. `"eth_signTypedData_v4"`)
/// - `params`   — JSON array of RPC params
/// - `chain_id` — wallet's currently active chain id (fallback when the
///   payload does not embed one)
///
/// # Errors
///
/// Returns [`SignResolveError`] when the method is unsupported, params are
/// malformed, or a required field is missing.
pub fn parse_sign_request(
    method: &str,
    params: &Value,
    chain_id: u64,
) -> Result<SignRequest, SignResolveError> {
    let sign_method = SignMethod::detect(method)
        .ok_or_else(|| SignResolveError::UnsupportedMethod(method.to_string()))?;

    let arr = params.as_array().ok_or(SignResolveError::ParamsNotArray)?;

    match sign_method {
        SignMethod::EthSignTypedDataV4 => parse_typed_data_v4(arr, chain_id),
        SignMethod::PersonalSign => parse_personal_sign(arr, chain_id),
        SignMethod::EthSign => parse_eth_sign(arr, chain_id),
        SignMethod::EthSignTransaction => parse_sign_transaction(arr, chain_id),
        SignMethod::EthSendUserOperation => parse_send_user_operation(arr, chain_id),
        SignMethod::WalletGrantPermissions => parse_wallet_grant_permissions(arr, chain_id),
    }
}

// ── per-method parsers ────────────────────────────────────────────────────────

/// `eth_signTypedData_v4`
/// params: [signerAddress, typedData (object or JSON string)]
fn parse_typed_data_v4(params: &[Value], chain_id: u64) -> Result<SignRequest, SignResolveError> {
    let signer = extract_address(params, 0)?;

    let raw = params.get(1).ok_or(SignResolveError::MissingParam(1))?;
    let typed_data = if let Some(s) = raw.as_str() {
        serde_json::from_str::<Value>(s)
            .map_err(|e| SignResolveError::InvalidTypedData(e.to_string()))?
    } else {
        raw.clone()
    };

    // Prefer domain.chainId over the wallet's current chain.
    let effective_chain_id = typed_data
        .get("domain")
        .and_then(|d| d.get("chainId"))
        .and_then(chain_id_from_value)
        .unwrap_or(chain_id);

    Ok(SignRequest {
        method: SignMethod::EthSignTypedDataV4,
        signer,
        chain_id: effective_chain_id,
        payload: SignPayload::TypedData(typed_data),
    })
}

/// `personal_sign`
/// params: [hexMessage, signerAddress]
fn parse_personal_sign(params: &[Value], chain_id: u64) -> Result<SignRequest, SignResolveError> {
    let message = params
        .first()
        .and_then(|v| v.as_str())
        .ok_or(SignResolveError::MissingParam(0))?
        .to_string();
    let signer = extract_address(params, 1)?;

    Ok(SignRequest {
        method: SignMethod::PersonalSign,
        signer,
        chain_id,
        payload: SignPayload::RawMessage(message),
    })
}

/// `eth_sign`
/// params: [signerAddress, hexHash]
fn parse_eth_sign(params: &[Value], chain_id: u64) -> Result<SignRequest, SignResolveError> {
    let signer = extract_address(params, 0)?;
    let hash = params
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or(SignResolveError::MissingParam(1))?
        .to_string();

    Ok(SignRequest {
        method: SignMethod::EthSign,
        signer,
        chain_id,
        payload: SignPayload::RawHash(hash),
    })
}

/// `eth_signTransaction`
/// params: [txObject { from, to, data, value, gas, chainId?, ... }]
fn parse_sign_transaction(
    params: &[Value],
    chain_id: u64,
) -> Result<SignRequest, SignResolveError> {
    let tx = params.first().ok_or(SignResolveError::MissingParam(0))?;

    let signer = tx
        .get("from")
        .and_then(|v| v.as_str())
        .map(normalize_address)
        .ok_or_else(|| SignResolveError::InvalidSigner("missing `from` in tx object".into()))?;

    // tx.chainId overrides the wallet's active chain when present.
    let effective_chain_id = tx
        .get("chainId")
        .and_then(chain_id_from_value)
        .unwrap_or(chain_id);

    Ok(SignRequest {
        method: SignMethod::EthSignTransaction,
        signer,
        chain_id: effective_chain_id,
        payload: SignPayload::Transaction(tx.clone()),
    })
}

/// `eth_sendUserOperation`
/// params: [userOpObject { sender, nonce, callData, ... }, entryPoint]
fn parse_send_user_operation(
    params: &[Value],
    chain_id: u64,
) -> Result<SignRequest, SignResolveError> {
    let user_op = params.first().ok_or(SignResolveError::MissingParam(0))?;

    let signer = user_op
        .get("sender")
        .and_then(|v| v.as_str())
        .map(normalize_address)
        .ok_or_else(|| SignResolveError::InvalidSigner("missing `sender` in UserOp".into()))?;

    let entry_point = params
        .get(1)
        .and_then(|v| v.as_str())
        .map(normalize_address)
        .unwrap_or_default();

    Ok(SignRequest {
        method: SignMethod::EthSendUserOperation,
        signer,
        chain_id,
        payload: SignPayload::UserOperation {
            user_op: user_op.clone(),
            entry_point,
        },
    })
}

/// `wallet_grantPermissions`
/// params: [permissionRequestObject { signer?, chainId?, ... }]
fn parse_wallet_grant_permissions(
    params: &[Value],
    chain_id: u64,
) -> Result<SignRequest, SignResolveError> {
    let request = params.first().ok_or(SignResolveError::MissingParam(0))?;

    let signer = request
        .get("signer")
        .or_else(|| request.get("address"))
        .and_then(|v| v.as_str())
        .map(normalize_address)
        .unwrap_or_default();

    let effective_chain_id = request
        .get("chainId")
        .and_then(chain_id_from_value)
        .unwrap_or(chain_id);

    Ok(SignRequest {
        method: SignMethod::WalletGrantPermissions,
        signer,
        chain_id: effective_chain_id,
        payload: SignPayload::PermissionRequest(request.clone()),
    })
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn extract_address(params: &[Value], index: usize) -> Result<String, SignResolveError> {
    params
        .get(index)
        .and_then(|v| v.as_str())
        .map(normalize_address)
        .ok_or(SignResolveError::MissingParam(index))
}

fn normalize_address(addr: &str) -> String {
    addr.to_lowercase()
}

/// Accept chainId as either a decimal integer or a `"0x…"` hex string.
fn chain_id_from_value(v: &Value) -> Option<u64> {
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    if let Some(s) = v.as_str() {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            return u64::from_str_radix(hex, 16).ok();
        }
        return s.parse::<u64>().ok();
    }
    None
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── eth_signTypedData_v4 ──────────────────────────────────────────────────

    #[test]
    fn typed_data_v4_extracts_signer_and_domain_chain_id() {
        let params = json!([
            "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B",
            {
                "domain": { "chainId": 1, "verifyingContract": "0x0000000000000000000000000000000000000001" },
                "primaryType": "Mail",
                "types": { "EIP712Domain": [], "Mail": [] },
                "message": {}
            }
        ]);
        let req = parse_sign_request("eth_signTypedData_v4", &params, 137).unwrap();
        assert_eq!(req.method, SignMethod::EthSignTypedDataV4);
        assert_eq!(req.signer, "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(req.chain_id, 1); // domain.chainId wins over wallet chain 137
    }

    #[test]
    fn typed_data_v4_falls_back_to_wallet_chain_id_when_no_domain_chain() {
        let params = json!([
            "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B",
            { "domain": {}, "primaryType": "X", "types": {}, "message": {} }
        ]);
        let req = parse_sign_request("eth_signTypedData_v4", &params, 42).unwrap();
        assert_eq!(req.chain_id, 42);
    }

    #[test]
    fn typed_data_v4_parses_json_string_payload() {
        let typed_data_str =
            r#"{"domain":{"chainId":10},"primaryType":"T","types":{},"message":{}}"#;
        let params = json!(["0x1111111111111111111111111111111111111111", typed_data_str]);
        let req = parse_sign_request("eth_signTypedData_v4", &params, 1).unwrap();
        assert_eq!(req.chain_id, 10);
    }

    #[test]
    fn typed_data_v4_accepts_hex_chain_id_in_domain() {
        let params = json!([
            "0x1111111111111111111111111111111111111111",
            { "domain": { "chainId": "0x89" }, "primaryType": "T", "types": {}, "message": {} }
        ]);
        let req = parse_sign_request("eth_signTypedData_v4", &params, 1).unwrap();
        assert_eq!(req.chain_id, 137); // 0x89 = 137
    }

    // ── personal_sign ─────────────────────────────────────────────────────────

    #[test]
    fn personal_sign_extracts_message_and_signer() {
        let params = json!(["0xdeadbeef", "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B"]);
        let req = parse_sign_request("personal_sign", &params, 1).unwrap();
        assert_eq!(req.method, SignMethod::PersonalSign);
        assert_eq!(req.signer, "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert!(matches!(req.payload, SignPayload::RawMessage(ref m) if m == "0xdeadbeef"));
    }

    // ── eth_sign ──────────────────────────────────────────────────────────────

    #[test]
    fn eth_sign_extracts_signer_and_hash() {
        let params = json!([
            "0x1111111111111111111111111111111111111111",
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ]);
        let req = parse_sign_request("eth_sign", &params, 1).unwrap();
        assert_eq!(req.method, SignMethod::EthSign);
        assert!(matches!(req.payload, SignPayload::RawHash(_)));
    }

    // ── eth_signTransaction ───────────────────────────────────────────────────

    #[test]
    fn sign_transaction_extracts_from_as_signer() {
        let params = json!([{
            "from": "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B",
            "to": "0x0000000000000000000000000000000000000002",
            "data": "0xabcdef",
            "value": "0x0",
            "chainId": 10
        }]);
        let req = parse_sign_request("eth_signTransaction", &params, 1).unwrap();
        assert_eq!(req.method, SignMethod::EthSignTransaction);
        assert_eq!(req.signer, "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(req.chain_id, 10); // tx.chainId wins
        assert!(matches!(req.payload, SignPayload::Transaction(_)));
    }

    #[test]
    fn sign_transaction_missing_from_returns_error() {
        let params = json!([{ "to": "0x0000000000000000000000000000000000000002" }]);
        let err = parse_sign_request("eth_signTransaction", &params, 1).unwrap_err();
        assert!(matches!(err, SignResolveError::InvalidSigner(_)));
    }

    // ── eth_sendUserOperation ─────────────────────────────────────────────────

    #[test]
    fn user_operation_extracts_sender_and_entry_point() {
        let params = json!([
            {
                "sender": "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B",
                "nonce": "0x0",
                "callData": "0x"
            },
            "0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789"
        ]);
        let req = parse_sign_request("eth_sendUserOperation", &params, 1).unwrap();
        assert_eq!(req.method, SignMethod::EthSendUserOperation);
        assert_eq!(req.signer, "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert!(matches!(
            req.payload,
            SignPayload::UserOperation { ref entry_point, .. }
            if entry_point == "0x5ff137d4b0fdcd49dca30c7cf57e578a026d2789"
        ));
    }

    // ── wallet_grantPermissions ───────────────────────────────────────────────

    #[test]
    fn wallet_grant_permissions_extracts_signer_and_chain_id() {
        let params = json!([{
            "signer": "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B",
            "chainId": 8453,
            "permissions": []
        }]);
        let req = parse_sign_request("wallet_grantPermissions", &params, 1).unwrap();
        assert_eq!(req.method, SignMethod::WalletGrantPermissions);
        assert_eq!(req.signer, "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(req.chain_id, 8453);
    }

    // ── error cases ───────────────────────────────────────────────────────────

    #[test]
    fn unsupported_method_returns_error() {
        let err = parse_sign_request("eth_sendTransaction", &json!([]), 1).unwrap_err();
        assert!(matches!(err, SignResolveError::UnsupportedMethod(_)));
    }

    #[test]
    fn non_array_params_returns_error() {
        let err = parse_sign_request("personal_sign", &json!({"a": 1}), 1).unwrap_err();
        assert_eq!(err, SignResolveError::ParamsNotArray);
    }
}
