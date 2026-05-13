//! `request-router`
//!
//! Single entry point for raw EVM RPC requests. Detects whether an incoming
//! request is a **sign** or **write** operation and dispatches accordingly:
//!
//! - **Sign** → [`sign_resolver::parse_sign_request`]
//!   For `eth_signTransaction` and `eth_sendUserOperation` the embedded
//!   `data` / `callData` field is additionally forwarded to `abi-resolver`
//!   so the inner calldata is decoded alongside the sign metadata.
//!
//! - **Write** → [`abi_resolver::resolver::Resolver::resolve`]
//!   Extracts `to` and `data` from the first params element and resolves
//!   the calldata through the tiered ABI lookup.
//!
//! Methods that are neither recognised sign methods nor parseable as a
//! write (no `to` address) return [`RouterOutput::Unsupported`].

use abi_resolver::resolver::{ResolveOutcome, Resolver};
use alloy_primitives::Address;
use serde_json::Value;
use sign_resolver::{parse_sign_request, SignMethod, SignPayload, SignRequest, SignResolveError};
use std::str::FromStr;

/// Output of a routing decision.
pub enum RouterOutput {
    Sign(SignRouterResult),
    Write(WriteRouterResult),
    /// Method is not a sign method and params could not be parsed as a write.
    Unsupported(String),
    Error(RouterError),
}

/// Result for a sign request.
pub struct SignRouterResult {
    /// Fully decoded sign request (method, signer, chain_id, payload).
    pub request: SignRequest,
    /// ABI-resolved calldata for `eth_signTransaction` and
    /// `eth_sendUserOperation`. `None` for all other sign methods, or when
    /// the embedded calldata is too short / unresolvable.
    pub calldata_resolved: Option<ResolveOutcome>,
}

/// Result for a write (transaction) request.
pub struct WriteRouterResult {
    pub from: Option<String>,
    pub to: String,
    pub value: String,
    /// ABI-resolver outcome for the transaction calldata.
    pub resolved: ResolveOutcome,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("sign parse error: {0}")]
    SignParse(#[from] SignResolveError),
    #[error("missing `to` address in params")]
    MissingTo,
    #[error("invalid address `{0}`")]
    InvalidAddress(String),
}

/// Route a raw RPC request to the appropriate decoder.
///
/// # Parameters
/// - `method`   — RPC method string (e.g. `"eth_sendTransaction"`)
/// - `params`   — JSON array of RPC params
/// - `chain_id` — wallet's currently active chain id
/// - `resolver` — pre-built ABI resolver (Sourcify + openchain tiers)
#[must_use]
pub fn route(method: &str, params: &Value, chain_id: u64, resolver: &Resolver) -> RouterOutput {
    if SignMethod::detect(method).is_some() {
        route_sign(method, params, chain_id, resolver)
    } else {
        route_write(params, chain_id, resolver)
    }
}

// ── sign path ─────────────────────────────────────────────────────────────────

fn route_sign(method: &str, params: &Value, chain_id: u64, resolver: &Resolver) -> RouterOutput {
    let request = match parse_sign_request(method, params, chain_id) {
        Ok(r) => r,
        Err(e) => return RouterOutput::Error(RouterError::SignParse(e)),
    };

    // Step 5: delegate inner callData to abi-resolver for tx / userOp.
    let calldata_resolved = resolve_inner_calldata(&request, resolver);

    RouterOutput::Sign(SignRouterResult {
        request,
        calldata_resolved,
    })
}

/// Extract the embedded calldata from sign payloads that carry a tx or UserOp,
/// then forward to abi-resolver. Returns `None` when not applicable or when
/// the calldata is absent / too short to decode.
fn resolve_inner_calldata(request: &SignRequest, resolver: &Resolver) -> Option<ResolveOutcome> {
    match &request.payload {
        // eth_signTransaction — `data` or `input` field is the calldata.
        SignPayload::Transaction(tx) => {
            let to_str = tx.get("to").and_then(|v| v.as_str())?;
            let data_hex = tx
                .get("data")
                .or_else(|| tx.get("input"))
                .and_then(|v| v.as_str())?;

            let address = Address::from_str(to_str).ok()?;
            let calldata = hex_to_bytes(data_hex)?;
            Some(resolver.resolve(request.chain_id, &address, &calldata))
        }

        // eth_sendUserOperation — `callData` is the encoded execution payload
        // sent to the sender (smart wallet) by the bundler.
        SignPayload::UserOperation { user_op, .. } => {
            let sender_str = user_op.get("sender").and_then(|v| v.as_str())?;
            let data_hex = user_op
                .get("callData")
                .or_else(|| user_op.get("calldata"))
                .and_then(|v| v.as_str())?;

            let address = Address::from_str(sender_str).ok()?;
            let calldata = hex_to_bytes(data_hex)?;
            Some(resolver.resolve(request.chain_id, &address, &calldata))
        }

        _ => None,
    }
}

// ── write path ────────────────────────────────────────────────────────────────

fn route_write(params: &Value, chain_id: u64, resolver: &Resolver) -> RouterOutput {
    match try_route_write(params, chain_id, resolver) {
        Ok(result) => RouterOutput::Write(result),
        Err(RouterError::MissingTo) => RouterOutput::Unsupported("no `to` in params".into()),
        Err(e) => RouterOutput::Error(e),
    }
}

fn try_route_write(
    params: &Value,
    chain_id: u64,
    resolver: &Resolver,
) -> Result<WriteRouterResult, RouterError> {
    let tx = params
        .as_array()
        .and_then(|a| a.first())
        .ok_or(RouterError::MissingTo)?;

    let to_str = tx
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or(RouterError::MissingTo)?;
    let address =
        Address::from_str(to_str).map_err(|_| RouterError::InvalidAddress(to_str.to_string()))?;

    let from = tx
        .get("from")
        .and_then(|v| v.as_str())
        .map(str::to_lowercase);
    let value = tx
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("0x0")
        .to_string();

    let calldata = tx
        .get("data")
        .or_else(|| tx.get("input"))
        .and_then(|v| v.as_str())
        .and_then(hex_to_bytes)
        .unwrap_or_default();

    let resolved = resolver.resolve(chain_id, &address, &calldata);

    Ok(WriteRouterResult {
        from,
        to: to_str.to_lowercase(),
        value,
        resolved,
    })
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let clean = hex
        .strip_prefix("0x")
        .or_else(|| hex.strip_prefix("0X"))
        .unwrap_or(hex);
    if clean.is_empty() {
        return Some(vec![]);
    }
    hex::decode(clean).ok()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use abi_resolver::resolver::ResolveOutcome;
    use serde_json::json;

    fn empty_resolver() -> Resolver {
        Resolver::empty()
    }

    // ── sign routing ──────────────────────────────────────────────────────────

    #[test]
    fn routes_typed_data_to_sign() {
        let params = json!([
            "0xab5801a7d398351b8be11c439e05c5b3259aec9b",
            { "domain": { "chainId": 1 }, "primaryType": "T", "types": {}, "message": {} }
        ]);
        let out = route("eth_signTypedData_v4", &params, 1, &empty_resolver());
        assert!(matches!(out, RouterOutput::Sign(_)));
    }

    #[test]
    fn routes_personal_sign_to_sign() {
        let params = json!(["0xdeadbeef", "0xab5801a7d398351b8be11c439e05c5b3259aec9b"]);
        let out = route("personal_sign", &params, 1, &empty_resolver());
        assert!(matches!(out, RouterOutput::Sign(_)));
    }

    #[test]
    fn sign_tx_has_no_calldata_resolved_when_resolver_empty() {
        let params = json!([{
            "from": "0xab5801a7d398351b8be11c439e05c5b3259aec9b",
            "to":   "0x0000000000000000000000000000000000000002",
            "data": "0x095ea7b3000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000003e8",
            "chainId": 1
        }]);
        let out = route("eth_signTransaction", &params, 1, &empty_resolver());
        let RouterOutput::Sign(result) = out else {
            panic!("expected Sign")
        };
        // empty resolver → NotFound, but the field is still populated
        assert!(matches!(
            result.calldata_resolved,
            Some(ResolveOutcome::NotFound)
        ));
    }

    #[test]
    fn user_op_calldata_is_forwarded_to_resolver() {
        let params = json!([
            {
                "sender": "0xab5801a7d398351b8be11c439e05c5b3259aec9b",
                "nonce":  "0x0",
                "callData": "0x095ea7b3000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000003e8"
            },
            "0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789"
        ]);
        let out = route("eth_sendUserOperation", &params, 1, &empty_resolver());
        let RouterOutput::Sign(result) = out else {
            panic!("expected Sign")
        };
        assert!(result.calldata_resolved.is_some());
    }

    #[test]
    fn typed_data_has_no_calldata_resolved() {
        let params = json!([
            "0xab5801a7d398351b8be11c439e05c5b3259aec9b",
            { "domain": {}, "primaryType": "T", "types": {}, "message": {} }
        ]);
        let out = route("eth_signTypedData_v4", &params, 1, &empty_resolver());
        let RouterOutput::Sign(result) = out else {
            panic!("expected Sign")
        };
        assert!(result.calldata_resolved.is_none());
    }

    // ── write routing ─────────────────────────────────────────────────────────

    #[test]
    fn routes_send_transaction_to_write() {
        let params = json!([{
            "from":  "0xab5801a7d398351b8be11c439e05c5b3259aec9b",
            "to":    "0x0000000000000000000000000000000000000002",
            "data":  "0x",
            "value": "0x0"
        }]);
        let out = route("eth_sendTransaction", &params, 1, &empty_resolver());
        assert!(matches!(out, RouterOutput::Write(_)));
    }

    #[test]
    fn write_result_lowercases_addresses() {
        let params = json!([{
            "from": "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B",
            "to":   "0x0000000000000000000000000000000000000002",
            "data": "0x"
        }]);
        let out = route("eth_sendTransaction", &params, 1, &empty_resolver());
        let RouterOutput::Write(result) = out else {
            panic!("expected Write")
        };
        assert_eq!(
            result.from.unwrap(),
            "0xab5801a7d398351b8be11c439e05c5b3259aec9b"
        );
    }

    // ── unsupported / error ───────────────────────────────────────────────────

    #[test]
    fn unknown_method_with_no_to_is_unsupported() {
        let out = route("eth_getBalance", &json!([]), 1, &empty_resolver());
        assert!(matches!(out, RouterOutput::Unsupported(_)));
    }

    #[test]
    fn sign_method_with_bad_params_returns_error() {
        // personal_sign with non-array params
        let out = route("personal_sign", &json!({"a": 1}), 1, &empty_resolver());
        assert!(matches!(out, RouterOutput::Error(_)));
    }
}
