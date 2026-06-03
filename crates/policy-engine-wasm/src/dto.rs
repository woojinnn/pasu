//! Serde-friendly DTOs for the WASM JSON boundary.

use serde::{Deserialize, Serialize};

use policy_engine::policy_rpc::PolicyManifest;
use policy_engine::schema::CustomFieldSource;

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
pub struct PreviewSchemaInputDto {
    #[serde(default)]
    pub manifests: Vec<PolicyManifest>,
}

// ───────────────────────────────────────────────────────────────────────────
// Declarative mapper boundary — install result (shared v3 / v1)
// ───────────────────────────────────────────────────────────────────────────

/// Result returned by `declarative_install_v3_json` on success.
#[derive(Debug, Clone, Serialize)]
pub struct DeclarativeInstallResultDto {
    /// Decoder id derived from the bundle. For v3 this equals `bundle_id`
    /// (the canonical registry path).
    pub decoder_id: String,
    /// Echoes back the bundle's full id (including `@version`) for client
    /// indexing.
    pub bundle_id: String,
}

// ───────────────────────────────────────────────────────────────────────────
// v3 route entry (raw tx / sig -> `Vec<Action>`)
// ───────────────────────────────────────────────────────────────────────────

/// Input for `declarative_route_request_v3_json`.
///
/// This is the v3 (PDF FSM spec) route entry that emits the new hierarchical
/// `policy_transition::action::Action` tree (the legacy flat
/// `ActionEnvelope` route was removed when the hierarchical action model
/// became the canonical route output).
///
/// The wire shape mirrors the SW orchestrator's [`decideMessage`] output:
///   * `chain_id`/`to`/`selector`/`calldata` — registry-v2 callkey + raw
///     calldata. (Mirrors the legacy v1 entry.)
///   * `value` — `msg.value` as a decimal string (`"0"` default).
///   * `gas_limit` — declared gas limit as a decimal string. The orchestrator
///     forwards the dApp's value verbatim; defaults to `"0"` when missing.
///   * `gas_price` — current gas price as a decimal string. The WASM route wraps
///     this in a [`LiveField`] with a Pyth `gas/<chain_id>` source — the
///     actual sync orchestrator wiring is deferred.
///   * `submitter` — `tx.from`. Echoed into `ActionMeta.submitter`.
///   * `submitted_at` — Unix epoch seconds. Echoed into `ActionMeta.submitted_at`.
///   * `nonce` — declared sequential nonce. `0` when missing.
///
/// `block_timestamp` (optional) — block.timestamp at which the Action would
/// land, distinct from `submitted_at`. Mappers may use this for deadlines.
///
/// `selector` and `block_timestamp` are part of the stable wire shape but are
/// not consumed by this route yet. They will be threaded into the registry-v2
/// callkey lookup and emit-rule decode once that path is fully wired. The
/// `#[allow(dead_code)]` reflects that intentional staging; do not remove
/// either field as that would break the SW wire layer.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct DeclarativeRouteRequestV3InputDto {
    pub chain_id: u64,
    /// "0x" + 40 hex. Case-insensitive.
    pub to: String,
    /// "0x" + 8 hex. Case-insensitive.
    pub selector: String,
    /// Raw "0x"-prefixed calldata.
    pub calldata: String,
    /// `msg.value` as a base-10 decimal string. Defaults to `"0"`.
    #[serde(default = "default_zero_decimal")]
    pub value: String,
    /// Declared gas limit as a base-10 decimal string. Defaults to `"0"`.
    #[serde(default = "default_zero_decimal")]
    pub gas_limit: String,
    /// Current gas price as a base-10 decimal string. Defaults to `"0"`.
    /// Wrapped in a [`LiveField`] by the WASM entry with a Pyth
    /// `gas/<chain_id>` source.
    #[serde(default = "default_zero_decimal")]
    pub gas_price: String,
    /// `tx.from` — "0x" + 40 hex.
    pub submitter: String,
    /// Unix epoch seconds at which the Action was submitted.
    pub submitted_at: u64,
    /// Sequential transaction nonce of `submitter`. Defaults to `0`.
    #[serde(default)]
    pub nonce: u64,
    /// Optional block timestamp.
    #[serde(default)]
    pub block_timestamp: Option<u64>,
}

fn default_zero_decimal() -> String {
    "0".to_string()
}

/// Input for `declarative_route_typed_data_v3_json` (Phase A.1).
///
/// The off-chain EIP-712 parallel to [`DeclarativeRouteRequestV3InputDto`].
/// Instead of raw calldata + selector, the wallet's `eth_signTypedData`
/// payload surfaces:
///   * `chain_id` / `verifying_contract` / `primary_type` — the typed-data
///     bridge key populated at install time from the manifest's
///     `match.typed_data` block. `verifying_contract` is case-insensitive.
///   * `domain_name` (optional) — the EIP-712 `domain.name`. Echoed verbatim
///     into the resulting [`Eip712Domain`](policy_transition::action::Eip712Domain).
///     Defaults to an empty string when the wallet payload omits it.
///   * `message` — the EIP-712 `message` object. The route handler reshapes
///     this into `args_json` via the ABI-derived wrap rule (single-tuple wrap
///     vs flat) so the manifest's `$args.<path>` placeholders resolve.
///   * `submitter` — the signer address. Echoed into `ActionMeta.submitter`.
///   * `submitted_at` — Unix epoch seconds. Echoed into `ActionMeta.submitted_at`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeclarativeRouteTypedDataV3InputDto {
    pub chain_id: u64,
    /// EIP-712 `domain.verifyingContract` — "0x" + 40 hex. Case-insensitive.
    pub verifying_contract: String,
    /// EIP-712 `primaryType` (e.g. `"PermitSingle"`).
    pub primary_type: String,
    /// Optional 4th routing-key component (T1). For Permit2
    /// `permitWitnessTransferFrom` witnesses (UniswapX intent orders etc.) the
    /// real order type is the EIP-712 `witness` field's type — every such order
    /// collides on `(chain_id, Permit2, "PermitWitnessTransferFrom")`, so
    /// `witness_type` (the witness struct's EIP-712 type name, kept VERBATIM
    /// like `primary_type`) disambiguates. Absent for non-witness payloads →
    /// the bridge key keeps its 3-tuple shape (backward compatible).
    #[serde(default)]
    pub witness_type: Option<String>,
    /// EIP-712 `domain.name`. Optional — defaults to empty.
    #[serde(default)]
    pub domain_name: Option<String>,
    /// The EIP-712 `message` object (decoded typed-data payload).
    pub message: serde_json::Value,
    /// Signer address — "0x" + 40 hex.
    pub submitter: String,
    /// Unix epoch seconds at which the signature was requested.
    pub submitted_at: u64,
}

/// Result returned by `declarative_route_request_v3_json` on success.
///
/// `actions` is the `Vec<policy_transition::action::Action>` produced for the
/// raw Tx — Phase 4B emits a single `ActionBody::Unknown` stub. `decoder_id`
/// echoes the bundle id when a registry match exists (Phase 4D+); empty
/// string when no match (stub fallback).
#[derive(Debug, Clone, Serialize)]
pub struct DeclarativeRouteRequestV3ResultDto {
    pub actions: Vec<policy_transition::action::Action>,
    pub decoder_id: String,
    /// When the matched manifest declares `emit.reenter_callback_arg`, the raw
    /// `bytes` value of that arg — an `abi.encode(Call[])` re-entry callback the
    /// caller (a `multicall_call_array` decode) recurses into. Generic: any
    /// bundler-adapter that nests a `Call[]` in a leg arg declares the arg name in
    /// its manifest, so the engine carries no per-protocol selector list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reenter_callback: Option<String>,
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
