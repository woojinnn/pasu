//! Thin wrappers over the WASM v3 route entrypoints.
//!
//! Build the route-input JSON exactly like
//! `crates/policy-engine-wasm/tests/declarative_v3_route.rs:42` and return the
//! parsed envelope `Value` (`{ok, data, error}`). Parse failures collapse to
//! `Value::Null`, which the oracle flags as a missing-`ok` envelope error.

use serde_json::{json, Value};

/// Deterministic fuzz submitter (`tx.from`). Arbitrary, fixed for reproducibility.
const FUZZ_SUBMITTER: &str = "0x000000000000000000000000000000000000aaaa";

/// Route a calldata transaction → parsed envelope.
#[must_use]
pub fn route_calldata(
    chain_id: u64,
    to: &str,
    selector: &str,
    calldata: &str,
    value: &str,
) -> Value {
    let input = json!({
        "chain_id": chain_id,
        "to": to,
        "selector": selector,
        "calldata": calldata,
        "value": value,
        "gas_limit": "200000",
        "gas_price": "20000000000",
        "submitter": FUZZ_SUBMITTER,
        "submitted_at": 1_700_000_000_u64,
        "nonce": 1_u64,
        "block_timestamp": 1_700_000_010_u64,
    });
    let out = policy_engine_wasm::declarative_route_request_v3_json(input.to_string());
    serde_json::from_str(&out).unwrap_or(Value::Null)
}

/// Route an EIP-712 typed-data signature → parsed envelope.
#[must_use]
pub fn route_typed_data(
    chain_id: u64,
    verifying_contract: &str,
    primary_type: &str,
    witness_type: Option<&str>,
    domain_name: Option<&str>,
    message: &Value,
) -> Value {
    let mut input = json!({
        "chain_id": chain_id,
        "verifying_contract": verifying_contract,
        "primary_type": primary_type,
        "message": message,
        "submitter": FUZZ_SUBMITTER,
        "submitted_at": 1_700_000_000_u64,
    });
    if let Some(w) = witness_type {
        input["witness_type"] = json!(w);
    }
    if let Some(d) = domain_name {
        input["domain_name"] = json!(d);
    }
    let out = policy_engine_wasm::declarative_route_typed_data_v3_json(input.to_string());
    serde_json::from_str(&out).unwrap_or(Value::Null)
}
