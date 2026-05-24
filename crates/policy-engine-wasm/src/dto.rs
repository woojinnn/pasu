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
    /// Phase 5: accept either the legacy `Vec<PolicyManifest>` shape or the
    /// new `{ [action]: PolicyManifest }` map. The map shape triggers the
    /// `compose_enriched` install path; the vec shape keeps the legacy
    /// behaviour for callers that have not migrated yet.
    #[serde(default)]
    pub manifests: ManifestsInputDto,
}

/// Wire shape for `install_policies_json` `manifests`.
///
/// **Phase 6 / D5 carry-over:** the two variants are NOT equivalent.
///
/// - [`ManifestsInputDto::Map`] (new, preferred) drives the
///   `compose_enriched` install path and produces an
///   [`InstallPoliciesOutputDto`] in the success envelope with
///   `enrichedSchemaHash` + per-action `addedCustomFields`.
/// - [`ManifestsInputDto::List`] (legacy) preserves the historical
///   `Vec<PolicyManifest>` install. It returns a `null` data envelope and
///   does **not** populate the enriched schema fields — callers that read
///   `enrichedSchemaHash` will silently see `undefined`.
///
/// **New Phase 6+ callers (browser-extension service worker manifest store,
/// dashboard SDK, anything that reads `enrichedSchemaHash`) MUST use the
/// Map shape.** The List shape exists only for the legacy
/// `policies-loader.ts` aggregator path and pre-Phase-5 plan/eval test
/// fixtures; it should not grow new consumers.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ManifestsInputDto {
    /// New, preferred shape: per-action map.
    ///
    /// Triggers `compose_enriched` and returns a populated
    /// [`InstallPoliciesOutputDto`] in the success envelope.
    Map(std::collections::BTreeMap<String, PolicyManifest>),
    /// Legacy shape: flat list. Skips `compose_enriched` and returns a
    /// null `data` envelope. Do NOT use this shape from new code.
    List(Vec<PolicyManifest>),
}

impl Default for ManifestsInputDto {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl ManifestsInputDto {
    /// Flatten to a `Vec<PolicyManifest>` for the legacy validators that take a slice.
    #[must_use]
    pub fn as_vec(&self) -> Vec<PolicyManifest> {
        match self {
            Self::Map(map) => map.values().cloned().collect(),
            Self::List(list) => list.clone(),
        }
    }

    /// Return the map shape when the caller used it; otherwise `None`.
    #[must_use]
    pub fn as_map(&self) -> Option<&std::collections::BTreeMap<String, PolicyManifest>> {
        match self {
            Self::Map(m) => Some(m),
            Self::List(_) => None,
        }
    }
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

/// Input for `evaluate_with_envelopes_json`.
///
/// Bypasses the route → plan stages by accepting envelopes that the caller
/// (e.g. the declarative pipeline in the orchestrator) has already produced.
/// `manifests` must match the manifests installed via `install_policies_json`
/// — the WASM enforces the same `manifest_set_hash` and `schema_hash`
/// equality as `evaluate_policy_rpc_json`.
///
/// `rpc_response` carries `policy-rpc` results when manifests declare any
/// `requires`; pass `{ "request_id": "...", "results": [] }` for pipelines
/// that do not need RPC enrichment (e.g. permit-only policies).
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateWithEnvelopesInputDto {
    pub envelopes: Vec<ActionEnvelope>,
    pub from: String,
    pub to: String,
    pub value_wei: String,
    pub chain_id: u64,
    #[serde(default)]
    pub block_timestamp: u64,
    #[serde(default)]
    pub manifests: Vec<PolicyManifest>,
    pub rpc_response: PolicyRpcResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewSchemaInputDto {
    #[serde(default)]
    pub manifests: Vec<PolicyManifest>,
}

// ───────────────────────────────────────────────────────────────────────────
// Declarative mapper boundary (Phase 1A)
// ───────────────────────────────────────────────────────────────────────────

/// Result returned by `declarative_install_json` on success.
#[derive(Debug, Clone, Serialize)]
pub struct DeclarativeInstallResultDto {
    /// Decoder id derived from the bundle (`declarative.<bundle.id-without-version>`).
    pub decoder_id: String,
    /// Echoes back the bundle's full id (including `@version`) for client
    /// indexing.
    pub bundle_id: String,
}

/// Input for `declarative_lookup_json`.
///
/// Phase 1A keeps this self-contained — it carries the decoder selection key
/// and a JSON-friendly `DecodedCall`. Bridge integration (selector → decoder)
/// is left for Phase 1B / Phase 2.
#[derive(Debug, Clone, Deserialize)]
pub struct DeclarativeLookupInputDto {
    /// Bundle's declarative decoder id (e.g.
    /// `"declarative.uniswap/v2/swapExactTokensForTokens"`).
    pub decoder_id: String,
    pub ctx: DeclarativeCtxDto,
    pub decoded: DecodedCallDto,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeclarativeCtxDto {
    pub chain_id: u64,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub value_wei: Option<String>,
    #[serde(default)]
    pub block_timestamp: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecodedCallDto {
    pub decoder_id: String,
    pub function_signature: String,
    #[serde(default)]
    pub args: Vec<DecodedArgDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecodedArgDto {
    pub name: String,
    pub abi_type: String,
    pub value: DecodedValueDto,
}

/// Tagged DTO for the calldata-decoder's value tree.
///
/// `kind` discriminates the variant. `value` payloads:
///   * `address`  — `"0x" + 40 hex` string.
///   * `uint`     — base-10 decimal string (lossless for `uint256`).
///   * `int`      — signed decimal string.
///   * `bool`     — boolean.
///   * `bytes`    — `"0x" + hex` string.
///   * `string`   — string.
///   * `array`    — array of `DecodedValueDto`.
///   * `tuple`    — array of `DecodedValueDto`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DecodedValueDto {
    Address(String),
    Uint(String),
    Int(String),
    Bool(bool),
    Bytes(String),
    String(String),
    Array(Vec<DecodedValueDto>),
    Tuple(Vec<DecodedValueDto>),
}

// ───────────────────────────────────────────────────────────────────────────
// Phase 6 — orchestrator route entry
// ───────────────────────────────────────────────────────────────────────────

/// Input for `declarative_route_request_json`.
///
/// `(chain_id, to, selector)` form the callkey for the bridge lookup. `ctx`
/// and `calldata` are the per-tx execution context and the raw calldata the
/// WASM decodes internally against the bridge-resolved bundle's
/// `abi_fragment.abi` (same pattern as `WasmChildResolver::resolve_child`).
#[derive(Debug, Clone, Deserialize)]
pub struct DeclarativeRouteRequestInputDto {
    pub chain_id: u64,
    /// "0x" + 40 hex. Case-insensitive — the bridge normalises to lowercase.
    pub to: String,
    /// "0x" + 8 hex. Case-insensitive — same as `to`.
    pub selector: String,
    pub ctx: DeclarativeCtxDto,
    /// Raw "0x"-prefixed calldata. WASM decodes it against the
    /// bridge-resolved bundle's abi_fragment.
    pub calldata: String,
}

/// Result returned by `declarative_route_request_json` on success.
/// `decoder_id` lets the caller correlate the envelopes with the bundle the
/// bridge resolved (useful for audit / telemetry).
#[derive(Debug, Clone, Serialize)]
pub struct DeclarativeRouteRequestResultDto {
    pub envelopes: Vec<policy_engine::ActionEnvelope>,
    pub decoder_id: String,
}

/// One child callkey produced by `declarative_plan_children_json`.
///
/// `to` echoes the outer request `to` — `self_array_bytes_last_arg` is a
/// self-multicall, so a child's `to` equals the outer `to`. `selector` is the
/// first 4 bytes of the child calldata as `"0x" + 8 hex`.
#[derive(Debug, Clone, Serialize)]
pub struct DeclarativeChildCallKeyDto {
    pub chain_id: u64,
    pub to: String,
    pub selector: String,
}

/// Result of `declarative_plan_children_json`.
///
/// `children` is empty when the outer bundle is not `multicall_recurse` (or no
/// bundle is mounted for the callkey) — the caller then skips the prefetch
/// pass. `decoder_id` echoes the outer bundle's declarative decoder id.
#[derive(Debug, Clone, Serialize)]
pub struct DeclarativePlanChildrenResultDto {
    pub children: Vec<DeclarativeChildCallKeyDto>,
    pub decoder_id: String,
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

/// `install_policies_json` success payload (Phase 5 extension).
///
/// Returned only when the caller provided the new `manifests` map shape. The
/// legacy list-shaped install path keeps emitting `null` `data`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPoliciesOutputDto {
    /// SHA-256 of the enriched cedarschema text.
    pub enriched_schema_hash: String,
    /// Per-action manifest-contributed fields keyed by `snake_case` action name.
    pub added_custom_fields: std::collections::BTreeMap<String, Vec<CustomFieldSource>>,
}

/// `preview_installed_schema_json` success payload (Phase 5 extension).
///
/// Keeps the legacy snake-case fields (`schema_text`, `schema_hash`,
/// `added_fields`) and additionally surfaces `customContexts` and
/// `schemaHash` (camelCase) when the installed schema was produced via the
/// manifests-map install path.
#[derive(Debug, Clone, Serialize)]
pub struct PreviewInstalledSchemaOutputDto {
    /// Final Cedar schema text.
    pub schema_text: String,
    /// SHA-256 of `schema_text` (snake-case alias of `schemaHash`).
    pub schema_hash: String,
    /// Legacy added-context-field summary.
    pub added_fields: Vec<policy_engine::schema::AddedContextField>,
    /// Per-action custom-context fields contributed by manifests (camelCase).
    #[serde(rename = "customContexts")]
    pub custom_contexts: std::collections::BTreeMap<String, Vec<CustomFieldSource>>,
    /// SHA-256 of the manifest-derived enriched cedarschema only.
    ///
    /// Equals `schema_hash` when no extra adapter fragment was concatenated.
    /// When the caller passed a non-empty `schema_text` to
    /// `install_policies_json`, the legacy `schema_hash` covers
    /// `enriched_text + "\n" + extra_schema_text` while this field still
    /// hashes only `enriched_text`. Phase 7's `/schema` viewer renders
    /// `enrichedSchemaText`, so the camelCase field is the one to display.
    #[serde(rename = "schemaHash")]
    pub schema_hash_camel: String,
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
