//! Manifest and wire DTOs for policy RPC.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;

/// Error type for manifest-driven policy RPC planning and materialization.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PolicyRpcError {
    /// Manifest input is malformed or unsupported.
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    /// Selector syntax or resolution failed.
    #[error("selector error: {0}")]
    Selector(String),
    /// RPC result data is missing or invalid.
    #[error("rpc result error: {0}")]
    RpcResult(String),
    /// Schema generation failed.
    #[error("schema error: {0}")]
    Schema(String),
    /// D9: a non-`optional` requirement failed to materialize (ok=false,
    /// missing payload, type-coercion failure, or absent result). The caller
    /// boundary must translate this into a synthetic `Verdict::Fail` carrying
    /// a `__system__` matched policy with reason `"rpc-unavailable: <call_id>"`.
    #[error("rpc unavailable for required requirement `{call_id}`: {reason}")]
    SystemFail {
        /// The originating policy-rpc call id (`manifest::index::requirement`).
        call_id: String,
        /// Human-readable cause (ok=false message, "missing payload", etc.).
        reason: String,
    },
}

/// Root transaction metadata used by selectors and WASM plan outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootInput {
    /// EVM chain id.
    pub chain_id: u64,
    /// Sender address.
    pub from: String,
    /// Target address.
    pub to: String,
    /// Native value as a decimal wei string.
    pub value_wei: String,
    /// Block timestamp used for policy evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_timestamp: Option<u64>,
}

impl RootInput {
    /// Convert to selector-friendly JSON.
    #[must_use]
    pub fn to_selector_json(&self) -> Value {
        let mut root = Map::new();
        root.insert("chain_id".into(), Value::from(self.chain_id));
        root.insert("from".into(), Value::from(self.from.as_str()));
        root.insert("to".into(), Value::from(self.to.as_str()));
        root.insert("value_wei".into(), Value::from(self.value_wei.as_str()));
        if let Some(block_timestamp) = self.block_timestamp {
            root.insert("block_timestamp".into(), Value::from(block_timestamp));
        }
        Value::Object(root)
    }
}

/// Policy bundle manifest fragment consumed by the WASM policy-rpc planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyManifest {
    /// Stable manifest id.
    pub id: String,
    /// Manifest schema version.
    pub schema_version: u64,
    /// RPC requirements declared by this manifest.
    #[serde(default)]
    pub requires: Vec<Requirement>,
    /// Context fields contributed by this manifest, keyed by action kind.
    #[serde(default)]
    pub context_extensions: BTreeMap<String, BTreeMap<String, String>>,
}

/// One conditional RPC method call requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    /// Requirement id, unique within a manifest.
    pub id: String,
    /// Action gate for this requirement.
    pub when: RequirementWhen,
    /// Opaque remote method name.
    pub method: String,
    /// Parameter template. String values beginning with `$.` are selectors.
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    /// Output projection rules.
    #[serde(default)]
    pub outputs: Vec<ContextProjection>,
    /// When true, a missing param selector silently skips this requirement
    /// instead of failing the whole plan. Use for enrichments whose source
    /// fields are conditionally present (e.g. `amount.value` is absent for
    /// `unlimited` amounts; `outputTokens` is absent for empty-only burns).
    #[serde(default)]
    pub optional: bool,
}

/// Requirement gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementWhen {
    /// Action kind, such as `swap`.
    pub action: String,
}

/// Output projection into a Cedar context field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProjection {
    /// Projection kind. V0 supports only `context`.
    pub kind: String,
    /// Destination context field.
    pub field: String,
    /// Destination Cedar type.
    #[serde(rename = "type")]
    pub type_name: ProjectionType,
    /// Selector rooted at `$.result`.
    pub from: String,
    /// Whether failure to project should fail closed.
    #[serde(default)]
    pub required: bool,
}

/// Supported v0 projection types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProjectionType {
    /// Cedar `String`.
    String,
    /// Cedar `Long`.
    Long,
    /// Cedar `Bool`.
    Bool,
    /// Cedar decimal extension value.
    Decimal,
    /// Cedar `UsdValuation`.
    UsdValuation,
    /// Cedar `WindowStats`.
    WindowStats,
    /// Cedar `Set<String>`.
    #[serde(rename = "Set<String>")]
    SetString,
}

impl ProjectionType {
    /// Return the Cedar schema spelling for this projection type.
    #[must_use]
    pub const fn cedar_type(&self) -> &'static str {
        match self {
            Self::String => "String",
            Self::Long => "Long",
            Self::Bool => "Bool",
            Self::Decimal => "decimal",
            Self::UsdValuation => "UsdValuation",
            Self::WindowStats => "WindowStats",
            Self::SetString => "Set<String>",
        }
    }
}

/// One planned policy RPC call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRpcCall {
    /// Call id.
    pub id: String,
    /// Remote method name.
    pub method: String,
    /// Remote method parameters.
    pub params: Value,
}

/// Batch policy-rpc response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRpcResponse {
    /// Request id echoed by the server.
    pub request_id: String,
    /// Per-call results.
    #[serde(default)]
    pub results: Vec<PolicyRpcResult>,
}

/// One policy-rpc result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRpcResult {
    /// Call id.
    pub id: String,
    /// Whether the call succeeded.
    pub ok: bool,
    /// Successful result payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Per-call error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<PolicyRpcErrorBody>,
}

/// Per-call policy-rpc error body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRpcErrorBody {
    /// Machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

/// Validate policy-rpc manifests.
///
/// # Errors
///
/// Returns an error when a manifest contains duplicate requirement ids.
pub fn validate_manifests(manifests: &[PolicyManifest]) -> Result<(), PolicyRpcError> {
    let mut manifest_ids = HashSet::new();
    for manifest in manifests {
        if !manifest_ids.insert(manifest.id.as_str()) {
            return Err(PolicyRpcError::InvalidManifest(format!(
                "duplicate manifest id `{}`",
                manifest.id
            )));
        }
        let mut ids = HashSet::new();
        for requirement in &manifest.requires {
            if !ids.insert(requirement.id.as_str()) {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "manifest `{}` has duplicate requirement id `{}`",
                    manifest.id, requirement.id
                )));
            }
        }
    }
    Ok(())
}
