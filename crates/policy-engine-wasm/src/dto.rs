//! Serde-friendly DTOs for the WASM JSON boundary.

use serde::{Deserialize, Serialize};

use policy_engine::policy_rpc::{PolicyManifest, PolicyRpcCall, PolicyRpcResponse, RootInput};
use policy_engine::ActionEnvelope;

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<EngineErrorDto>,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(EngineErrorDto::new(kind, message)),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialization cannot fail")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineErrorDto {
    pub kind: String,
    pub message: String,
}

impl EngineErrorDto {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InstallPoliciesInputDto {
    #[serde(default)]
    pub schema_text: String,
    pub policy_set: Vec<PolicyEntryDto>,
    #[serde(default)]
    pub manifests: Vec<PolicyManifest>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyEntryDto {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerdictDto {
    Pass,
    Warn { matched: Vec<MatchedPolicyDto> },
    Fail { matched: Vec<MatchedPolicyDto> },
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchedPolicyDto {
    pub policy_id: String,
    pub reason: Option<String>,
    pub severity: String,
    pub origin: String,
}

// `HostSnapshotDto` and its entry types are kept for the JSON wire shape only.
// The new pipeline does not yet consume the snapshot (oracle/balances/allowances/
// windows are tier-1/tier-2 facts that the rebuilt host capabilities layer will
// rewire later). For now we still accept the same input shape from TS callers so
// the boundary contract is stable — hence `#[allow(dead_code)]` on the fields.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HostSnapshotDto {
    #[serde(default)]
    pub oracle: Vec<OracleEntryDto>,
    #[serde(default)]
    pub balances: Vec<BalanceEntryDto>,
    #[serde(default)]
    pub allowances: Vec<AllowanceEntryDto>,
    #[serde(default)]
    pub now_ts: Option<u64>,
    #[serde(default)]
    pub windows: Vec<WindowEntryDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateEnvelopeInputDto {
    pub envelope: serde_json::Value,
    pub from: String,
    pub to: String,
    pub value_wei: String,
    pub chain_id: u64,
    pub block_timestamp: u64,
    #[allow(dead_code)]
    pub host_snapshot: HostSnapshotDto,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawRequestDto {
    pub method: String,
    pub params: serde_json::Value,
    pub chain_id: u64,
    #[serde(default)]
    pub block_timestamp: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanPolicyRpcInputDto {
    pub request_id: String,
    pub raw_request: RawRequestDto,
    #[serde(default)]
    pub manifests: Vec<PolicyManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRpcPlanDto {
    pub request_id: String,
    pub root: RootInput,
    pub envelopes: Vec<ActionEnvelope>,
    pub calls: Vec<PolicyRpcCall>,
    pub manifest_set_hash: String,
    pub schema_hash: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvaluatePolicyRpcInputDto {
    pub plan: PolicyRpcPlanDto,
    pub rpc_response: PolicyRpcResponse,
    #[serde(default)]
    pub manifests: Vec<PolicyManifest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewSchemaInputDto {
    #[serde(default)]
    pub manifests: Vec<PolicyManifest>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct OracleEntryDto {
    pub token_key: String,
    pub usd_per_unit: String,
    pub as_of_ts: u64,
    #[serde(default)]
    pub stale_sec: u64,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BalanceEntryDto {
    pub owner: String,
    pub token_key: String,
    pub balance: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AllowanceEntryDto {
    pub owner: String,
    pub token_key: String,
    pub spender: String,
    pub allowance: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct WindowEntryDto {
    pub actor: String,
    pub name: String,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn envelope_ok_uses_boolean_wire_shape() {
        let output = Envelope::ok(json!({"answer": 42})).to_json();
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["answer"], 42, "{parsed}");
        assert!(parsed["error"].is_null(), "{parsed}");
    }
}
