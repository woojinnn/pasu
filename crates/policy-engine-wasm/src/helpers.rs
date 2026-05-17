//! Pure parsing helpers exposed to JS.
//! JS performs routing/dispatch on top of these.

use serde::Serialize;
use serde_json::json;
use sign_resolver::{parse_sign_request, SignResolveError};
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SignParseOutcome {
    Ok {
        method: String,
        signer: String,
        chain_id: u64,
        payload: serde_json::Value,
    },
    Err {
        message: String,
    },
}

/// Parse an `eth_signTypedData_v4` / `personal_sign` / `eth_sign` /
/// `eth_signTransaction` / `eth_sendUserOperation` / `wallet_grantPermissions`
/// RPC into a `SignRequest`-shaped JSON. Returns an outcome tagged with
/// `kind: "ok" | "err"`.
#[wasm_bindgen]
pub fn parse_sign_request_json(method: &str, params_json: &str, chain_id: u64) -> String {
    let params: serde_json::Value = match serde_json::from_str(params_json) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&SignParseOutcome::Err {
                message: format!("params parse: {e}"),
            })
            .unwrap();
        }
    };
    match parse_sign_request(method, &params, chain_id) {
        Ok(req) => serde_json::to_string(&SignParseOutcome::Ok {
            method: format!("{:?}", req.method).to_lowercase(),
            signer: req.signer,
            chain_id: req.chain_id,
            payload: serde_json::to_value(&req.payload).unwrap_or(serde_json::Value::Null),
        })
        .unwrap(),
        Err(e) => serde_json::to_string(&SignParseOutcome::Err {
            message: error_message(&e),
        })
        .unwrap(),
    }
}

fn error_message(e: &SignResolveError) -> String {
    e.to_string()
}

/// Structural framing of arbitrary calldata. No ABI lookup — just splits
/// `selector` (first 4 bytes) and `body_hex` (rest). The JS adapter-loader
/// uses this when the registry has no entry for `(chainId, address)`.
#[wasm_bindgen]
pub fn decode_abi_standard_json(calldata_hex: &str) -> String {
    let raw = calldata_hex.strip_prefix("0x").unwrap_or(calldata_hex);
    let bytes = match hex::decode(raw) {
        Ok(b) => b,
        Err(_) => return json!({ "error": "bad_hex" }).to_string(),
    };
    if bytes.len() < 4 {
        return json!({
            "selector": format!("0x{}", hex::encode(&bytes)),
            "body_hex": "",
        })
        .to_string();
    }
    let selector = &bytes[..4];
    let body = &bytes[4..];
    json!({
        "selector": format!("0x{}", hex::encode(selector)),
        "body_hex": hex::encode(body),
    })
    .to_string()
}
