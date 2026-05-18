//! Serde-friendly DTOs for the WASM JSON boundary.

use serde::{Deserialize, Serialize};

use policy_engine::policy_rpc::{PolicyManifest, PolicyRpcCall, PolicyRpcResponse, RootInput};
use policy_engine::schema::CustomFieldSource;
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

/// One entry in the base alias table surfaced through `get_alias_table_json`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AliasEntryDto {
    /// Manifest-facing alias name.
    pub name: String,
    /// `"scalar"` or `"record"`.
    pub kind: String,
    /// Cedar source spelling.
    pub cedar_spelling: String,
}

/// `get_alias_table_json` success payload.
#[derive(Debug, Clone, Serialize)]
pub struct AliasTableOutput {
    /// Alias entries.
    pub entries: Vec<AliasEntryDto>,
}

/// `preview_custom_schema_json` input shape: a single `{action, manifest}` pair.
#[derive(Debug, Clone, Deserialize)]
pub struct PreviewCustomSchemaInputDto {
    /// Target action (snake_case).
    pub action: String,
    /// Manifest contributing the action's custom context fields.
    pub manifest: PolicyManifest,
}

/// One entry in `preview_custom_schema_json` `customTypes` array.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTypeDto {
    /// Action name (`snake_case`).
    pub name: String,
    /// Fields contributed by the manifest for this action.
    pub fields: Vec<CustomFieldSource>,
}

/// One side of the `D14` per-action diff.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomSchemaDiffDto {
    /// Fields present in the previewed manifest but missing from the
    /// currently-installed enriched schema for the action.
    pub added: Vec<CustomFieldSource>,
    /// Fields present in the installed schema but absent from the preview.
    pub removed: Vec<CustomFieldSource>,
    /// Fields whose name matches but whose `cedar_type` differs.
    pub changed: Vec<CustomFieldChangeDto>,
}

/// A single changed-field entry: the same `field` with two different types.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldChangeDto {
    /// Field name shared by both sides.
    pub field: String,
    /// Cedar type currently installed for the action (or empty on first install).
    pub installed_cedar_type: String,
    /// Cedar type the previewed manifest would produce.
    pub preview_cedar_type: String,
}

/// `preview_custom_schema_json` success payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCustomSchemaOutputDto {
    /// Per-action custom types contributed by the previewed manifest.
    pub custom_types: Vec<CustomTypeDto>,
    /// Full enriched cedarschema text after merging the previewed manifest
    /// with the bundled base.
    pub enriched_schema_text: String,
    /// Per-action `D14` diff against the currently-installed enriched schema.
    pub diff: CustomSchemaDiffDto,
    /// SHA-256 of `enriched_schema_text`.
    pub schema_hash: String,
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
