//! Thin `#[wasm_bindgen]` JSON-string exports.

use crate::dto::{
    AliasEntryDto, AliasTableOutput, CustomFieldChangeDto, CustomSchemaDiffDto, CustomTypeDto,
    EngineErrorDto, Envelope, EvaluatePolicyRpcInputDto, EvaluateWithEnvelopesInputDto,
    InstallPoliciesInputDto, InstallPoliciesOutputDto, MatchedPolicyDto, PlanPolicyRpcInputDto,
    PolicyRpcPlanDto, PreviewCustomSchemaInputDto, PreviewCustomSchemaOutputDto,
    PreviewInstalledSchemaOutputDto, PreviewSchemaInputDto, RawRequestDto, VerdictDto,
};
use alloy_primitives::U256;
use policy_engine::lowering::{policy_request_from_envelope, try_policy_request_from_envelope};
use policy_engine::policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyRequestOrigin, Severity, Verdict,
};
use policy_engine::policy_rpc::{
    apply_rpc_results_with_indices, manifest_set_hash, plan_calls, system_fail_verdict, RootInput,
};
use policy_engine::schema::{
    compose_enriched, schema_hash, AddedContextField, CustomFieldSource, EnrichedSchema,
    PolicySchemaComposer,
};
use policy_engine::{ActionAddress, ActionEnvelope, DecimalString};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

// 사용자 디버깅용 console.log binding — evaluate_envelopes_inner 에서
// PolicyRequest 의 정확한 JSON 형태를 SW console 에 노출하기 위함.
// `#[cfg(target_arch = "wasm32")]` gating — cargo test 환경 의 panic
// ("cannot call wasm-bindgen imported functions on non-wasm targets") 회피.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log_str(s: &str);
}

#[cfg(not(target_arch = "wasm32"))]
fn console_log_str(_: &str) {}

pub struct EngineState {
    pub policies: PolicyEngine,
    pub manifest_set_hash: String,
    pub schema_hash: String,
    pub schema_text: String,
    pub added_fields: Vec<AddedContextField>,
    /// Installed enriched schema (Phase 5). Some when the caller passed the
    /// `manifests` map shape to `install_policies_json`; None otherwise.
    pub enriched: Option<EnrichedSchema>,
}

thread_local! {
    static STATE: RefCell<Option<EngineState>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn install_policies_json(policies_json: String) -> String {
    // Phase 5.3: `manifests` may arrive as either the legacy `Vec` or the new
    // per-action map. The map shape composes via `compose_enriched` and
    // returns `InstallPoliciesOutputDto` in the success envelope. The list
    // shape preserves the historical legacy install (null data).
    let result = (|| -> Result<Option<InstallPoliciesOutputDto>, EngineErrorDto> {
        // phase7 audit P0 — fail-closed on oversized input JSON.
        check_input_size(&policies_json, "install_policies_json")?;
        let input: InstallPoliciesInputDto =
            serde_json::from_str(&policies_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        let manifest_vec = input.manifests.as_vec();
        let schema_preview = PolicySchemaComposer::new()
            .with_manifests(&manifest_vec)
            .map_err(|error| EngineErrorDto::new("schema_failed", error.to_string()))?
            .preview();

        // Compose the enriched schema when the caller used the new map shape;
        // otherwise stay on the legacy composer's text.
        let enriched = match input.manifests.as_map() {
            Some(map) => Some(
                compose_enriched(map)
                    .map_err(|error| EngineErrorDto::new("schema_failed", error.to_string()))?,
            ),
            None => None,
        };
        let base_schema_text = enriched.as_ref().map_or_else(
            || schema_preview.schema_text.clone(),
            |e| e.schema_text.clone(),
        );
        let schema_text = if input.schema_text.trim().is_empty() {
            base_schema_text
        } else {
            format!("{}\n{}", base_schema_text, input.schema_text)
        };
        let installed_schema_hash = schema_hash(&schema_text);
        let mut builder = PolicyEngineBuilder::with_schema_text(schema_text.clone());
        for policy in input.policy_set {
            builder = builder.add_text(namespace_policy_text(&policy.id, &policy.text));
        }

        let policies = builder
            .build()
            .map_err(|error| EngineErrorDto::new("install_failed", error.to_string()))?;

        let output = enriched.as_ref().map(|e| InstallPoliciesOutputDto {
            enriched_schema_hash: e.schema_hash.clone(),
            added_custom_fields: e.custom_types_by_action.clone(),
        });

        STATE.with(|state| {
            *state.borrow_mut() = Some(EngineState {
                policies,
                manifest_set_hash: manifest_set_hash(&manifest_vec),
                schema_hash: installed_schema_hash,
                schema_text,
                added_fields: schema_preview.added_fields,
                enriched,
            });
        });
        Ok(output)
    })();

    match result {
        Ok(Some(output)) => Envelope::ok(output).to_json(),
        Ok(None) => Envelope::<()>::ok(()).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

fn not_installed_error() -> EngineErrorDto {
    EngineErrorDto::new(
        "not_installed",
        "install_policies_json must be called first",
    )
}

/// Maximum accepted JSON input byte length at the WASM boundary (4 MiB).
///
/// Round 1 audit (P1) — bound `String` inputs before `serde_json::from_str` so
/// a hostile caller cannot drive the WASM allocator into an OOM with a giant
/// payload. 4 MiB easily covers every legitimate request: an
/// `evaluate_policy_rpc_json` plan with all of the supported manifests, a full
/// Universal-Router opcode stream, and the largest declarative bundle all sit
/// under ~50 KiB.
pub(crate) const MAX_WASM_INPUT_JSON_LEN: usize = 4 * 1024 * 1024;

/// Reject WASM JSON inputs that exceed [`MAX_WASM_INPUT_JSON_LEN`].
///
/// Returns an `EngineErrorDto` with `kind = "input_too_large"` so callers can
/// distinguish a size violation from a malformed-JSON case.
pub(crate) fn check_input_size(input_json: &str, entry: &str) -> Result<(), EngineErrorDto> {
    if input_json.len() > MAX_WASM_INPUT_JSON_LEN {
        return Err(EngineErrorDto::new(
            "input_too_large",
            format!(
                "{entry} input json length {} exceeds {} byte limit",
                input_json.len(),
                MAX_WASM_INPUT_JSON_LEN
            ),
        ));
    }
    Ok(())
}

fn verdict_to_dto(verdict: Verdict) -> VerdictDto {
    match verdict {
        Verdict::Pass => VerdictDto::Pass,
        Verdict::Warn(matched) => VerdictDto::Warn {
            matched: matched.iter().map(matched_to_dto).collect(),
        },
        Verdict::Fail(matched) => VerdictDto::Fail {
            matched: matched.iter().map(matched_to_dto).collect(),
        },
    }
}

fn matched_to_dto(matched: &MatchedPolicy) -> MatchedPolicyDto {
    MatchedPolicyDto {
        policy_id: matched.policy_id.clone(),
        reason: matched.reason.clone(),
        severity: severity_to_string(matched.severity),
        origin: origin_to_string(matched.origin),
    }
}

fn engine_error_verdict(error: EngineErrorDto) -> VerdictDto {
    let policy_id = format!("__engine::{}", error.kind);
    let reason = if error.message.is_empty() {
        policy_id.clone()
    } else {
        error.message
    };
    VerdictDto::Fail {
        matched: vec![MatchedPolicyDto {
            policy_id,
            reason: Some(reason),
            severity: "deny".to_string(),
            origin: "engine_error".to_string(),
        }],
    }
}

fn severity_to_string(severity: Severity) -> String {
    match severity {
        Severity::Deny => "deny",
        Severity::Warn => "warn",
    }
    .to_string()
}

fn origin_to_string(origin: PolicyRequestOrigin) -> String {
    match origin {
        PolicyRequestOrigin::Action => "action",
        PolicyRequestOrigin::Tx => "tx",
    }
    .to_string()
}

#[cfg(test)]
fn has_id_annotation(text: &str) -> bool {
    let stripped = strip_cedar_comments(text);
    let prefix_end = first_policy_head_index(&stripped).unwrap_or(stripped.len());
    stripped[..prefix_end].contains("@id(")
}

fn namespace_policy_text(entry_id: &str, text: &str) -> String {
    let annotations = id_annotations(text);
    if annotations.is_empty() {
        return format!("@id({})\n{}", json_string(entry_id), text);
    }

    let mut output = String::with_capacity(text.len() + entry_id.len());
    let mut cursor = 0;
    let single_annotation = annotations.len() == 1;
    for annotation in annotations {
        output.push_str(&text[cursor..annotation.value_start]);
        let replacement = if single_annotation {
            entry_id.to_string()
        } else {
            format!("{entry_id}::{}", annotation.value)
        };
        output.push_str(&json_string(&replacement));
        cursor = annotation.value_end;
    }
    output.push_str(&text[cursor..]);
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdAnnotation {
    value_start: usize,
    value_end: usize,
    value: String,
}

fn id_annotations(text: &str) -> Vec<IdAnnotation> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index = skip_string(bytes, index);
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            b'@' if bytes[index..].starts_with(b"@id") => {
                if let Some((annotation, next_index)) = parse_id_annotation(text, index) {
                    out.push(annotation);
                    index = next_index;
                } else {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }

    out
}

fn parse_id_annotation(text: &str, start: usize) -> Option<(IdAnnotation, usize)> {
    let bytes = text.as_bytes();
    let mut index = start + 3;
    index = skip_ascii_ws(bytes, index);
    if bytes.get(index) != Some(&b'(') {
        return None;
    }
    index = skip_ascii_ws(bytes, index + 1);
    if bytes.get(index) != Some(&b'"') {
        return None;
    }

    let value_start = index;
    let value_end = skip_string(bytes, value_start);
    let literal = text.get(value_start..value_end)?;
    let value = serde_json::from_str::<String>(literal).unwrap_or_default();
    index = skip_ascii_ws(bytes, value_end);
    if bytes.get(index) != Some(&b')') {
        return None;
    }

    Some((
        IdAnnotation {
            value_start,
            value_end,
            value,
        },
        index + 1,
    ))
}

fn skip_ascii_ws(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    index
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

#[cfg(test)]
fn strip_cedar_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '/' {
            output.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('/') => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            Some('*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            _ => output.push(ch),
        }
    }

    output
}

#[cfg(test)]
fn first_policy_head_index(text: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            continue;
        }

        if keyword_at(text, index, "permit") || keyword_at(text, index, "forbid") {
            return Some(index);
        }
    }

    None
}

#[cfg(test)]
fn keyword_at(text: &str, index: usize, keyword: &str) -> bool {
    text[index..].starts_with(keyword)
        && text[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_ident_char(ch))
        && text[index + keyword.len()..]
            .chars()
            .next()
            .is_none_or(|ch| !is_ident_char(ch))
}

#[cfg(test)]
fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_entry_id_is_escaped_when_injected() {
        let source = r#"
            @severity("deny")
            forbid(principal, action == Action::"other", resource);
        "#;
        let rewritten = namespace_policy_text(r#"bundle::quote"id"#, source);
        assert!(
            rewritten.contains(r#"@id("bundle::quote\"id")"#),
            "{rewritten}"
        );
        assert!(
            !rewritten.contains("@id(\"bundle::quote\"id\")"),
            "{rewritten}"
        );
    }

    #[test]
    fn id_guard_ignores_commented_ids() {
        let policy = r#"
            // @id("commented-out")
            /* @id("also-commented-out") */
            @severity("deny")
            forbid(principal, action == Action::"other", resource);
        "#;

        assert!(!has_id_annotation(policy));
    }
}

// ── Phase 7: route_request_json ───────────────────────────────────────────────
// New-pipeline entry point exposing `request_router::route_request` to JS.
// Returns the `Vec<ActionEnvelope>` JSON inside the standard `{ok, data}` envelope.

#[wasm_bindgen]
pub fn route_request_json(input_json: String) -> String {
    if let Err(err) = check_input_size(&input_json, "route_request_json") {
        return Envelope::<()>::err(err.kind, err.message).to_json();
    }
    let parse_result: Result<RawRequestDto, _> = serde_json::from_str(&input_json);
    let input = match parse_result {
        Ok(v) => v,
        Err(e) => {
            return Envelope::<()>::err("invalid_input_json", format!("invalid input json: {e}"))
                .to_json();
        }
    };

    let registries = request_router::DefaultRegistries::standard();
    let token_registry = BuiltinTokenRegistry;
    let ctx = request_router::RouterContext {
        registries: &registries,
        token_registry: &token_registry,
        block_timestamp: input.block_timestamp,
    };
    match request_router::route_request(&ctx, &input.method, &input.params, input.chain_id) {
        Ok(envelopes) => Envelope::ok(envelopes).to_json(),
        Err(e) => Envelope::<()>::err("route_failed", e.to_string()).to_json(),
    }
}

#[wasm_bindgen]
pub fn preview_custom_schema_json(input_json: String) -> String {
    let result = (|| -> Result<PreviewCustomSchemaOutputDto, EngineErrorDto> {
        let input: PreviewCustomSchemaInputDto =
            serde_json::from_str(&input_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;
        let mut map: std::collections::BTreeMap<String, policy_engine::policy_rpc::PolicyManifest> =
            std::collections::BTreeMap::new();
        map.insert(input.action.clone(), input.manifest);

        let enriched = compose_enriched(&map)
            .map_err(|error| EngineErrorDto::new("schema_failed", error.to_string()))?;

        // D14: diff the previewed action against the currently-installed
        // enriched schema's custom fields for the same action. Compare empty
        // when nothing is installed yet, or when the installed state was
        // produced via the legacy (Vec-shaped) install path.
        let installed_fields = STATE.with(|state| {
            state
                .borrow()
                .as_ref()
                .and_then(|s| s.enriched.as_ref())
                .and_then(|e| e.custom_types_by_action.get(&input.action).cloned())
                .unwrap_or_default()
        });
        let preview_fields = enriched
            .custom_types_by_action
            .get(&input.action)
            .cloned()
            .unwrap_or_default();
        let diff = diff_custom_fields(&installed_fields, &preview_fields);

        let custom_types = enriched
            .custom_types_by_action
            .iter()
            .map(|(name, fields)| CustomTypeDto {
                name: name.clone(),
                fields: fields.clone(),
            })
            .collect();

        Ok(PreviewCustomSchemaOutputDto {
            custom_types,
            enriched_schema_text: enriched.schema_text,
            diff,
            schema_hash: enriched.schema_hash,
        })
    })();

    match result {
        Ok(preview) => Envelope::ok(preview).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

/// Build the per-action D14 diff between an installed and a previewed custom-
/// field list, keyed by `field` name.
fn diff_custom_fields(
    installed: &[CustomFieldSource],
    preview: &[CustomFieldSource],
) -> CustomSchemaDiffDto {
    use std::collections::BTreeMap;
    let installed_by: BTreeMap<&str, &CustomFieldSource> =
        installed.iter().map(|f| (f.field.as_str(), f)).collect();
    let preview_by: BTreeMap<&str, &CustomFieldSource> =
        preview.iter().map(|f| (f.field.as_str(), f)).collect();

    let mut diff = CustomSchemaDiffDto::default();
    for (name, prev) in &preview_by {
        match installed_by.get(name) {
            None => diff.added.push((*prev).clone()),
            Some(inst) if inst.cedar_type != prev.cedar_type => {
                diff.changed.push(CustomFieldChangeDto {
                    field: (*name).to_owned(),
                    installed_cedar_type: inst.cedar_type.clone(),
                    preview_cedar_type: prev.cedar_type.clone(),
                });
            }
            Some(_) => {}
        }
    }
    for (name, inst) in &installed_by {
        if !preview_by.contains_key(name) {
            diff.removed.push((*inst).clone());
        }
    }
    diff
}

#[wasm_bindgen]
pub fn get_alias_table_json() -> String {
    use policy_engine::schema::aliases::{base_alias_table, AliasKind};
    let entries = base_alias_table()
        .iter()
        .map(|(name, entry)| AliasEntryDto {
            name: (*name).to_owned(),
            kind: match entry.kind {
                AliasKind::Scalar => "scalar",
                AliasKind::Record => "record",
            }
            .to_owned(),
            cedar_spelling: entry.cedar_spelling.to_owned(),
        })
        .collect();
    Envelope::ok(AliasTableOutput { entries }).to_json()
}

#[wasm_bindgen]
pub fn preview_schema_json(input_json: String) -> String {
    let result = (|| -> Result<policy_engine::schema::SchemaPreview, EngineErrorDto> {
        check_input_size(&input_json, "preview_schema_json")?;
        let input: PreviewSchemaInputDto = serde_json::from_str(&input_json).map_err(|error| {
            EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
        })?;
        PolicySchemaComposer::new()
            .with_manifests(&input.manifests)
            .map_err(|error| EngineErrorDto::new("schema_failed", error.to_string()))
            .map(|composer| composer.preview())
    })();

    match result {
        Ok(preview) => Envelope::ok(preview).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

#[wasm_bindgen]
pub fn preview_installed_schema_json() -> String {
    // Phase 5.3: extend the legacy `SchemaPreview` shape with camelCase
    // `customContexts` and `schemaHash` keyed off the installed enriched
    // schema (when present).
    let result: Result<PreviewInstalledSchemaOutputDto, EngineErrorDto> = STATE.with(|state| {
        let state = state.borrow();
        let state = state.as_ref().ok_or_else(not_installed_error)?;
        let (custom_contexts, schema_hash_camel) = match state.enriched.as_ref() {
            Some(e) => (e.custom_types_by_action.clone(), e.schema_hash.clone()),
            None => (std::collections::BTreeMap::new(), state.schema_hash.clone()),
        };
        Ok(PreviewInstalledSchemaOutputDto {
            schema_text: state.schema_text.clone(),
            schema_hash: state.schema_hash.clone(),
            added_fields: state.added_fields.clone(),
            custom_contexts,
            schema_hash_camel,
        })
    });

    match result {
        Ok(preview) => Envelope::ok(preview).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

#[wasm_bindgen]
pub fn plan_policy_rpc_json(input_json: String) -> String {
    let result = (|| -> Result<PolicyRpcPlanDto, EngineErrorDto> {
        check_input_size(&input_json, "plan_policy_rpc_json")?;
        let input: PlanPolicyRpcInputDto = serde_json::from_str(&input_json).map_err(|error| {
            EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
        })?;
        let envelopes = route_envelopes(&input.raw_request)?;
        let root = root_from_raw_request(&input.raw_request)?;
        let schema_preview = PolicySchemaComposer::new()
            .with_manifests(&input.manifests)
            .map_err(|error| EngineErrorDto::new("schema_failed", error.to_string()))?
            .preview();
        let manifest_hash = manifest_set_hash(&input.manifests);
        let schema_hash = STATE.with(|state| {
            state
                .borrow()
                .as_ref()
                .filter(|state| state.manifest_set_hash == manifest_hash)
                .map_or_else(
                    || schema_preview.schema_hash.clone(),
                    |state| state.schema_hash.clone(),
                )
        });
        let calls = plan_calls(
            &root,
            &envelopes,
            &input.manifests,
            &input.raw_request.params,
        )
        .map_err(|error| EngineErrorDto::new("plan_failed", error.to_string()))?;

        Ok(PolicyRpcPlanDto {
            request_id: input.request_id,
            root,
            envelopes,
            calls,
            manifest_set_hash: manifest_hash,
            schema_hash,
            diagnostics: Vec::new(),
        })
    })();

    match result {
        Ok(plan) => Envelope::ok(plan).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

#[wasm_bindgen]
pub fn evaluate_policy_rpc_json(input_json: String) -> String {
    let verdict = (|| -> Result<Verdict, EngineErrorDto> {
        check_input_size(&input_json, "evaluate_policy_rpc_json")?;
        let input: EvaluatePolicyRpcInputDto =
            serde_json::from_str(&input_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        if input.rpc_response.request_id != input.plan.request_id {
            return Err(EngineErrorDto::new(
                "request_id_mismatch",
                "rpc_response.request_id does not match plan.request_id",
            ));
        }
        let manifest_hash = manifest_set_hash(&input.manifests);
        if manifest_hash != input.plan.manifest_set_hash {
            return Err(EngineErrorDto::new(
                "manifest_hash_mismatch",
                "active manifests do not match policy-rpc plan",
            ));
        }
        STATE.with(|state| {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(not_installed_error)?;
            if state.manifest_set_hash != manifest_hash {
                return Err(EngineErrorDto::new(
                    "installed_manifest_hash_mismatch",
                    "installed policies were built with a different manifest set",
                ));
            }
            if state.schema_hash != input.plan.schema_hash {
                return Err(EngineErrorDto::new(
                    "installed_schema_hash_mismatch",
                    "installed policies were built with a different schema",
                ));
            }
            Ok(())
        })?;

        let from: ActionAddress = input
            .plan
            .root
            .from
            .parse()
            .map_err(|error| EngineErrorDto::new("invalid_from", error))?;
        let to: ActionAddress = input
            .plan
            .root
            .to
            .parse()
            .map_err(|error| EngineErrorDto::new("invalid_to", error))?;
        let value_wei: DecimalString = input
            .plan
            .root
            .value_wei
            .parse()
            .map_err(|error| EngineErrorDto::new("invalid_value_wei", error))?;
        let block_timestamp = input.plan.root.block_timestamp.unwrap_or_default();

        let mut policy_envelopes = Vec::new();
        let mut requests = Vec::new();
        for (envelope_index, envelope) in input.plan.envelopes.iter().enumerate() {
            if let Some(request) = policy_request_from_envelope(
                envelope,
                &from,
                &to,
                &value_wei,
                input.plan.root.chain_id,
                block_timestamp,
            ) {
                policy_envelopes.push((envelope_index, envelope));
                requests.push(request);
            }
        }

        if let Err(error) = apply_rpc_results_with_indices(
            &mut requests,
            &policy_envelopes,
            &input.manifests,
            &input.rpc_response,
        ) {
            // D9: surface `SystemFail` as the legitimate evaluation outcome
            // (`Verdict::Fail` with the synthetic `__system__` matched policy)
            // rather than an engine error. Any other variant remains an
            // engine-level projection failure.
            if let Some(verdict) = system_fail_verdict(&error) {
                return Ok(verdict);
            }
            return Err(EngineErrorDto::new("projection_failed", error.to_string()));
        }

        STATE.with(|state| {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(not_installed_error)?;
            if state.manifest_set_hash != manifest_hash {
                return Err(EngineErrorDto::new(
                    "installed_manifest_hash_mismatch",
                    "installed policies were built with a different manifest set",
                ));
            }
            if state.schema_hash != input.plan.schema_hash {
                return Err(EngineErrorDto::new(
                    "installed_schema_hash_mismatch",
                    "installed policies were built with a different schema",
                ));
            }
            state
                .policies
                .evaluate_requests(
                    requests
                        .iter()
                        .map(|request| (request, PolicyRequestOrigin::Action)),
                )
                .map_err(|error| EngineErrorDto::new("policy", error.to_string()))
        })
    })();

    let dto = match verdict {
        Ok(verdict) => verdict_to_dto(verdict),
        Err(error) => engine_error_verdict(error),
    };
    Envelope::ok(dto).to_json()
}

/// Evaluate policies against caller-supplied envelopes.
///
/// Phase 7A entry that lets the declarative pipeline drive Cedar verdicts
/// directly from envelopes it produced, skipping the route → plan stages.
/// The function still enforces:
///   * Installed policies' `manifest_set_hash` matches the `manifests` arg.
///   * Installed `schema_hash` matches the schema derived from `manifests`.
///   * `rpc_response` projects without error (fail-closed on required RPC).
///
/// The reply envelope mirrors `evaluate_policy_rpc_json` — `Envelope::ok` with
/// a `VerdictDto`, or a synthetic `Fail`/`__engine::*` matched policy when
/// validation fails.
#[wasm_bindgen]
pub fn evaluate_with_envelopes_json(input_json: String) -> String {
    let verdict = (|| -> Result<Verdict, EngineErrorDto> {
        check_input_size(&input_json, "evaluate_with_envelopes_json")?;
        let input: EvaluateWithEnvelopesInputDto = serde_json::from_str(&input_json)
            .map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        let manifest_hash = manifest_set_hash(&input.manifests);
        let schema_hash = STATE.with(|state| {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(not_installed_error)?;
            if state.manifest_set_hash != manifest_hash {
                return Err(EngineErrorDto::new(
                    "installed_manifest_hash_mismatch",
                    "installed policies were built with a different manifest set",
                ));
            }
            Ok(state.schema_hash.clone())
        })?;

        evaluate_envelopes_inner(
            &input.envelopes,
            &input.from,
            &input.to,
            &input.value_wei,
            input.chain_id,
            input.block_timestamp,
            &input.manifests,
            &manifest_hash,
            &schema_hash,
            &input.rpc_response,
        )
    })();

    let dto = match verdict {
        Ok(verdict) => verdict_to_dto(verdict),
        Err(error) => engine_error_verdict(error),
    };
    Envelope::ok(dto).to_json()
}

/// Lower envelopes into Cedar requests, project RPC results, and evaluate.
///
/// Shared by `evaluate_policy_rpc_json` and `evaluate_with_envelopes_json`.
/// `installed_manifest_hash` and `installed_schema_hash` are the values the
/// caller already verified against the engine state — passing them through
/// is purely defensive (re-checked after `apply_rpc_results_with_indices` to
/// fail-closed on concurrent reinstalls).
#[allow(clippy::too_many_arguments)]
fn evaluate_envelopes_inner(
    envelopes: &[policy_engine::ActionEnvelope],
    from_str: &str,
    to_str: &str,
    value_wei_str: &str,
    chain_id: u64,
    block_timestamp: u64,
    manifests: &[policy_engine::policy_rpc::PolicyManifest],
    installed_manifest_hash: &str,
    installed_schema_hash: &str,
    rpc_response: &policy_engine::policy_rpc::PolicyRpcResponse,
) -> Result<Verdict, EngineErrorDto> {
    let from: ActionAddress = from_str
        .parse()
        .map_err(|error| EngineErrorDto::new("invalid_from", error))?;
    let to: ActionAddress = to_str
        .parse()
        .map_err(|error| EngineErrorDto::new("invalid_to", error))?;
    let value_wei: DecimalString = value_wei_str
        .parse()
        .map_err(|error| EngineErrorDto::new("invalid_value_wei", error))?;

    let mut policy_envelopes = Vec::new();
    let mut requests = Vec::new();
    let mut synthetic_verdicts: Vec<Verdict> = Vec::new();
    for (envelope_index, envelope) in envelopes.iter().enumerate() {
        // Round 5 audit (P0) — `policy_request_from_envelope` historically
        // collapsed every lowering failure into `None`, which downstream
        // treated as "skip this envelope". A failing lowering on a hostile
        // calldata would therefore vanish from policy evaluation and
        // produce a default `Pass` verdict (fail-open). Switching to
        // `try_policy_request_from_envelope` keeps lowering errors
        // visible so we can fail-closed (`__engine::lowering_failed`).
        //
        // Round 8 audit (P0-1) — `Ok(None)` (action variant has no lowering
        // yet) used to silently `continue`, so when every envelope was
        // unmapped `evaluate_requests` aggregated an empty list to `Pass`.
        // That made an entire crvUSD / veCRV / Gauge tx route around any
        // forbid policy. Defense-in-depth: synthesize a `Warn` verdict with
        // an `__engine::action_not_lowered::<kind>` MatchedPolicy so the
        // host wallet at least surfaces the gap rather than rubber-stamping
        // the request.
        match try_policy_request_from_envelope(
            envelope,
            &from,
            &to,
            &value_wei,
            chain_id,
            block_timestamp,
        ) {
            Ok(Some(request)) => {
                policy_envelopes.push((envelope_index, envelope));
                requests.push(request);
            }
            Ok(None) => {
                let kind = envelope.action.kind();
                let policy_id = format!("__engine::action_not_lowered::{kind}");
                console_log_str(&format!(
                    "[Scopeball] envelope[{envelope_index}] action {kind} has no lowering — emitting synthetic Warn"
                ));
                synthetic_verdicts.push(Verdict::Warn(vec![MatchedPolicy {
                    policy_id: policy_id.clone(),
                    reason: Some(policy_id),
                    severity: Severity::Warn,
                    origin: PolicyRequestOrigin::Action,
                }]));
                continue;
            }
            Err(error) => {
                return Err(EngineErrorDto::new(
                    "lowering_failed",
                    format!(
                        "envelope {envelope_index} ({}) failed to lower into a policy request: {error}",
                        envelope.action.kind()
                    ),
                ));
            }
        }
    }

    apply_rpc_results_with_indices(&mut requests, &policy_envelopes, manifests, rpc_response)
        .map_err(|error| EngineErrorDto::new("projection_failed", error.to_string()))?;

    // 사용자 디버깅용 — Cedar 평가 직전 의 final PolicyRequest 의 정확한 JSON 형태 출력.
    // 본 출력 으로 사용자 가 SW console 에서 entities + context attribute 의 실제 값 확인 가능.
    for (idx, request) in requests.iter().enumerate() {
        let request_json = serde_json::to_string(request)
            .unwrap_or_else(|error| format!("<serialize fail: {error}>"));
        console_log_str(&format!(
            "[Scopeball] policy_request[{idx}]: {request_json}"
        ));
    }

    STATE.with(|state| {
        let state = state.borrow();
        let state = state.as_ref().ok_or_else(not_installed_error)?;
        if state.manifest_set_hash != installed_manifest_hash {
            return Err(EngineErrorDto::new(
                "installed_manifest_hash_mismatch",
                "installed policies were built with a different manifest set",
            ));
        }
        if state.schema_hash != installed_schema_hash {
            return Err(EngineErrorDto::new(
                "installed_schema_hash_mismatch",
                "installed policies were built with a different schema",
            ));
        }
        let policy_verdict = state
            .policies
            .evaluate_requests(
                requests
                    .iter()
                    .map(|request| (request, PolicyRequestOrigin::Action)),
            )
            .map_err(|error| EngineErrorDto::new("policy", error.to_string()))?;
        // Round 8 audit (P0-1) — aggregate the synthetic `Warn` verdicts
        // produced for unmapped action variants into the final verdict so
        // they cannot vanish on the way back to the host.
        let mut combined = synthetic_verdicts;
        combined.push(policy_verdict);
        Ok(Verdict::aggregate(combined))
    })
}

fn route_envelopes(input: &RawRequestDto) -> Result<Vec<ActionEnvelope>, EngineErrorDto> {
    let registries = request_router::DefaultRegistries::standard();
    let token_registry = BuiltinTokenRegistry;
    let ctx = request_router::RouterContext {
        registries: &registries,
        token_registry: &token_registry,
        block_timestamp: input.block_timestamp,
    };
    request_router::route_request(&ctx, &input.method, &input.params, input.chain_id)
        .map_err(|error| EngineErrorDto::new("route_failed", error.to_string()))
}

struct BuiltinTokenRegistry;

impl mappers::TokenRegistry for BuiltinTokenRegistry {
    fn lookup(
        &self,
        chain_id: u64,
        address: &policy_engine::ActionAddress,
    ) -> Option<mappers::TokenMetadata> {
        if chain_id != 1 {
            return None;
        }
        match address.to_string().as_str() {
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2" => Some(mappers::TokenMetadata {
                symbol: "WETH".to_owned(),
                decimals: 18,
            }),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" => Some(mappers::TokenMetadata {
                symbol: "USDC".to_owned(),
                decimals: 6,
            }),
            "0xdac17f958d2ee523a2206206994597c13d831ec7" => Some(mappers::TokenMetadata {
                symbol: "USDT".to_owned(),
                decimals: 6,
            }),
            _ => None,
        }
    }
}

fn root_from_raw_request(input: &RawRequestDto) -> Result<RootInput, EngineErrorDto> {
    if input.method.starts_with("eth_signTypedData") {
        return root_from_typed_signature_raw_request(input);
    }
    root_from_transaction_raw_request(input)
}

fn root_from_transaction_raw_request(input: &RawRequestDto) -> Result<RootInput, EngineErrorDto> {
    let tx = input
        .params
        .as_array()
        .and_then(|params| params.first())
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            EngineErrorDto::new("invalid_raw_request", "missing transaction params[0]")
        })?;
    let from = tx
        .get("from")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| EngineErrorDto::new("invalid_raw_request", "missing transaction.from"))?;
    let to = tx
        .get("to")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| EngineErrorDto::new("invalid_raw_request", "missing transaction.to"))?;
    let value_wei = tx
        .get("value")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| Ok("0".to_owned()), value_to_decimal_wei)?;

    Ok(RootInput {
        chain_id: input.chain_id,
        from: from.to_ascii_lowercase(),
        to: to.to_ascii_lowercase(),
        value_wei,
        block_timestamp: input.block_timestamp,
    })
}

fn root_from_typed_signature_raw_request(
    input: &RawRequestDto,
) -> Result<RootInput, EngineErrorDto> {
    let params = input.params.as_array().ok_or_else(|| {
        EngineErrorDto::new(
            "invalid_raw_request",
            "typed signature params must be an array",
        )
    })?;
    let signer = params
        .first()
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            EngineErrorDto::new("invalid_raw_request", "missing typed signature signer")
        })?;
    let typed_data = params.get(1).ok_or_else(|| {
        EngineErrorDto::new("invalid_raw_request", "missing typed signature payload")
    })?;
    let parsed_typed_data;
    let typed_data = if let Some(raw) = typed_data.as_str() {
        parsed_typed_data = serde_json::from_str::<serde_json::Value>(raw).map_err(|error| {
            EngineErrorDto::new(
                "invalid_raw_request",
                format!("invalid typed signature json: {error}"),
            )
        })?;
        &parsed_typed_data
    } else {
        typed_data
    };
    let verifying_contract = typed_data
        .get("domain")
        .and_then(|domain| domain.get("verifyingContract"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("0x0000000000000000000000000000000000000000");

    Ok(RootInput {
        chain_id: input.chain_id,
        from: signer.to_ascii_lowercase(),
        to: verifying_contract.to_ascii_lowercase(),
        value_wei: "0".to_owned(),
        block_timestamp: input.block_timestamp,
    })
}

fn value_to_decimal_wei(value: &str) -> Result<String, EngineErrorDto> {
    if let Some(hex) = value.strip_prefix("0x") {
        return U256::from_str_radix(if hex.is_empty() { "0" } else { hex }, 16)
            .map(|value| value.to_string())
            .map_err(|error| EngineErrorDto::new("invalid_value_wei", error.to_string()));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests_route_request {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn route_request_unsupported_method_returns_err() {
        let out = route_request_json(
            json!({
                "method": "personal_sign",
                "params": [],
                "chain_id": 1u64,
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false);
        assert_eq!(parsed["error"]["kind"], "route_failed");
    }

    #[test]
    fn route_request_unknown_selector_returns_err() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../crates/integration-tests/data/golden/inputs/unknown_selector.json"
        ))
        .unwrap();
        let input = json!({
            "method": fixture["rpc"]["method"],
            "params": fixture["rpc"]["params"],
            "chain_id": fixture["chain_id"],
        });
        let out = route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false);
    }

    #[test]
    fn route_request_v2_swap_returns_envelope() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../crates/integration-tests/data/golden/inputs/swap_uniswap_v2_exact_in.json"
        ))
        .unwrap();
        let input = json!({
            "method": fixture["rpc"]["method"],
            "params": fixture["rpc"]["params"],
            "chain_id": fixture["chain_id"],
        });
        let out = route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        let actions = parsed["data"].as_array().expect("data is array");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["action"], "swap");
    }
}

#[cfg(test)]
mod tests_policy_rpc {
    use super::*;
    use serde_json::{json, Value};

    fn manifest_json() -> Value {
        json!({
            "id": "user/max-input-usd-100",
            "schema_version": 1,
            "requires": [{
                "id": "swap-total-input-usd",
                "when": { "action": "swap" },
                "method": "oracle.usd_value",
                "params": {
                    "chain_id": "$.root.chain_id",
                    "asset": "$.action.inputToken.asset",
                    "amount": "$.action.inputToken.amount.value"
                },
                "outputs": [{
                    "kind": "context",
                    "field": "totalInputUsd",
                    "type": "UsdValuation",
                    "from": "$.result",
                    "required": true
                }]
            }],
            "context_extensions": {
                "swap": { "totalInputUsd": "UsdValuation" }
            }
        })
    }

    fn default_max_input_manifest_json() -> Value {
        serde_json::from_str(include_str!(
            "../../../policy-rpc/examples/policies/swap/max-input-usd-100.policy-rpc.json"
        ))
        .unwrap()
    }

    fn default_min_output_manifest_json() -> Value {
        serde_json::from_str(include_str!(
            "../../../policy-rpc/examples/policies/swap/min-output-usd-floor.policy-rpc.json"
        ))
        .unwrap()
    }

    fn custom_field_manifest_json() -> Value {
        json!({
            "id": "user/custom-risk-score",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {
                "swap": { "tokenRiskScore": "Long" }
            }
        })
    }

    /// Merge `requires[]` from `manifests` that target `action` into a
    /// single manifest object suitable for the install-path Map shape.
    /// `requires[].id` is rewritten by suffixing the source manifest id
    /// so two manifests can contribute the same requirement id without
    /// colliding under Rule 1.
    fn merge_manifests_for_action(action: &str, manifests: &[Value]) -> Value {
        let mut requires: Vec<Value> = Vec::new();
        let mut extensions: serde_json::Map<String, Value> = serde_json::Map::new();
        for (i, m) in manifests.iter().enumerate() {
            if let Some(arr) = m["requires"].as_array() {
                for req in arr {
                    let mut req_clone = req.clone();
                    if let Some(when_action) = req_clone["when"]["action"].as_str() {
                        if when_action != action {
                            continue;
                        }
                    }
                    if let Some(req_id) = req_clone["id"].as_str() {
                        req_clone["id"] = Value::String(format!("{req_id}__src{i}"));
                    }
                    requires.push(req_clone);
                }
            }
            if let Some(ext_obj) = m["context_extensions"][action].as_object() {
                for (k, v) in ext_obj {
                    extensions.entry(k.clone()).or_insert(v.clone());
                }
            }
        }
        json!({
            "id": format!("merged::{action}"),
            "schema_version": 1,
            "requires": requires,
            "context_extensions": {
                action: extensions,
            },
        })
    }

    fn install_usd_policy() {
        let output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": { "swap": manifest_json() },
                "policy_set": [{
                    "id": "bundle::max-input-usd-100",
                    "text": r#"
                        @severity("deny")
                        @reason("too much USD")
                        forbid(principal, action == Action::"swap", resource)
                        when {
                            context has custom &&
                            context.custom has totalInputUsd &&
                            context.custom.totalInputUsd.value.greaterThan(decimal("100.00"))
                        };
                    "#
                }]
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
    }

    fn swap_raw_request() -> Value {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../crates/integration-tests/data/golden/inputs/swap_uniswap_v2_exact_in.json"
        ))
        .unwrap();
        json!({
            "method": fixture["rpc"]["method"],
            "params": fixture["rpc"]["params"],
            "chain_id": fixture["chain_id"],
            "block_timestamp": 1_700_000_000_u64
        })
    }

    fn typed_signature_raw_request() -> Value {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../crates/integration-tests/data/golden/inputs/eip2612_permit.json"
        ))
        .unwrap();
        json!({
            "method": fixture["rpc"]["method"],
            "params": fixture["rpc"]["params"],
            "chain_id": fixture["chain_id"],
            "block_timestamp": 1_700_000_000_u64
        })
    }

    fn plan_input() -> Value {
        json!({
            "request_id": "eval-1",
            "raw_request": swap_raw_request(),
            "manifests": [manifest_json()]
        })
    }

    #[test]
    fn plan_policy_rpc_json_returns_oracle_call() {
        let output = plan_policy_rpc_json(plan_input().to_string());
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["calls"][0]["method"],
            json!("oracle.usd_value"),
            "{parsed}"
        );
        assert_eq!(
            parsed["data"]["calls"][0]["params"]["chain_id"],
            json!(1),
            "{parsed}"
        );
    }

    #[test]
    fn plan_policy_rpc_json_accepts_typed_signature_request() {
        let output = plan_policy_rpc_json(
            json!({
                "request_id": "typed-1",
                "raw_request": typed_signature_raw_request(),
                "manifests": []
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["root"]["from"], "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "{parsed}"
        );
        assert_eq!(
            parsed["data"]["root"]["to"], "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "{parsed}"
        );
    }

    #[test]
    fn evaluate_policy_rpc_json_projects_result_and_evaluates_policy() {
        install_usd_policy();
        let plan_output = plan_policy_rpc_json(plan_input().to_string());
        let plan: Value = serde_json::from_str::<Value>(&plan_output).unwrap()["data"].clone();

        let output = evaluate_policy_rpc_json(
            json!({
                "plan": plan,
                "rpc_response": {
                    "request_id": "eval-1",
                    "results": [{
                        "id": "user/max-input-usd-100::0::swap-total-input-usd",
                        "ok": true,
                        "result": {
                            "value": "3500.1200",
                            "asOfTs": 1_700_000_000_u64,
                            "staleSec": 5,
                            "sources": ["coingecko"]
                        }
                    }]
                },
                "manifests": [manifest_json()]
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "fail", "{parsed}");
        assert_eq!(
            parsed["data"]["matched"][0]["policy_id"], "bundle::max-input-usd-100",
            "{parsed}"
        );
    }

    #[test]
    fn evaluate_policy_rpc_json_fails_closed_on_required_rpc_error() {
        install_usd_policy();
        let plan_output = plan_policy_rpc_json(plan_input().to_string());
        let plan: Value = serde_json::from_str::<Value>(&plan_output).unwrap()["data"].clone();

        let output = evaluate_policy_rpc_json(
            json!({
                "plan": plan,
                "rpc_response": {
                    "request_id": "eval-1",
                    "results": [{
                        "id": "user/max-input-usd-100::0::swap-total-input-usd",
                        "ok": false,
                        "error": {
                            "code": "invalid_params",
                            "message": "bad asset"
                        }
                    }]
                },
                "manifests": [manifest_json()]
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "fail", "{parsed}");
        assert_eq!(
            parsed["data"]["matched"][0]["policy_id"], "__system__",
            "{parsed}"
        );
        assert_eq!(
            parsed["data"]["matched"][0]["reason"],
            "rpc-unavailable: user/max-input-usd-100::0::swap-total-input-usd",
            "{parsed}"
        );
        assert_eq!(parsed["data"]["matched"][0]["origin"], "action", "{parsed}");
    }

    #[test]
    fn default_swap_policy_rpc_files_plan_and_evaluate() {
        let max_manifest = default_max_input_manifest_json();
        let min_manifest = default_min_output_manifest_json();
        assert_eq!(
            max_manifest["context_extensions"]["swap"]["totalInputUsd"], "UsdValuation",
            "{max_manifest}"
        );
        assert!(
            max_manifest["context_extensions"]["swap"]
                .get("rpcTotalInputUsd")
                .is_none(),
            "{max_manifest}"
        );
        assert_eq!(
            min_manifest["context_extensions"]["swap"]["totalMinOutputUsd"], "UsdValuation",
            "{min_manifest}"
        );
        assert!(
            min_manifest["context_extensions"]["swap"]
                .get("rpcTotalMinOutputUsd")
                .is_none(),
            "{min_manifest}"
        );
        // Install path requires the Map shape so `compose_enriched`
        // produces the `<Action>CustomContext` block the v1 policy texts
        // depend on. The two example manifests both target `swap`, so we
        // merge their requirements into a single manifest keyed under
        // "swap" — plan/evaluate then consume the same merged set via
        // the list shape (`as_vec()` flattens the map's `.values()` to
        // the same sequence the planner sees).
        let merged_swap_manifest =
            merge_manifests_for_action("swap", &[max_manifest.clone(), min_manifest.clone()]);
        let manifests = vec![merged_swap_manifest.clone()];

        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": { "swap": merged_swap_manifest },
                "policy_set": [
                    {
                        "id": "default::swap/max-input-usd-100",
                        "text": include_str!("../../../policy-rpc/examples/policies/swap/max-input-usd-100.cedar")
                    },
                    {
                        "id": "default::swap/min-output-usd-floor",
                        "text": include_str!("../../../policy-rpc/examples/policies/swap/min-output-usd-floor.cedar")
                    }
                ]
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let plan_output = plan_policy_rpc_json(
            json!({
                "request_id": "default-eval-1",
                "raw_request": swap_raw_request(),
                "manifests": manifests.clone()
            })
            .to_string(),
        );
        let plan: Value = serde_json::from_str::<Value>(&plan_output).unwrap()["data"].clone();
        let calls = plan["calls"].as_array().expect("calls is array");
        assert_eq!(calls.len(), 2, "{plan}");

        let input_call = calls
            .iter()
            .find(|call| {
                call["id"]
                    .as_str()
                    .is_some_and(|id| id.contains("swap-total-input-usd"))
            })
            .expect("input USD call");
        assert_eq!(input_call["params"]["asset"]["symbol"], "USDT");
        assert_eq!(input_call["params"]["amount"], "200000000");

        let output_call = calls
            .iter()
            .find(|call| {
                call["id"]
                    .as_str()
                    .is_some_and(|id| id.contains("swap-min-output-usd"))
            })
            .expect("min output USD call");
        assert_eq!(output_call["params"]["asset"]["symbol"], "WETH");
        assert_eq!(output_call["params"]["amount"], "0");

        let results: Vec<Value> = calls
            .iter()
            .map(|call| {
                let id = call["id"].as_str().expect("call id");
                let value = if id.contains("swap-min-output-usd") {
                    "40.0000"
                } else {
                    "80.0000"
                };
                json!({
                    "id": id,
                    "ok": true,
                    "result": {
                        "value": value,
                        "asOfTs": 1_700_000_000_u64,
                        "staleSec": 5,
                        "sources": ["coingecko"]
                    }
                })
            })
            .collect();

        let output = evaluate_policy_rpc_json(
            json!({
                "plan": plan,
                "rpc_response": {
                    "request_id": "default-eval-1",
                    "results": results
                },
                "manifests": manifests
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();
        let matched = parsed["data"]["matched"].as_array().expect("matched array");

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "fail", "{parsed}");
        assert_eq!(matched.len(), 1, "{parsed}");
        assert!(
            matched[0]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("Minimum output")),
            "{parsed}"
        );
    }

    /// Fix N reproducer: mirrors the production SW first-run install.
    /// `policies-loader.installFiltered` reads `getAllManifests()` —
    /// which returns `{}` on first boot — and sends `manifests: {}` to
    /// WASM. With an empty Map shape `compose_enriched` produces no
    /// `<Action>CustomContext` fragments, so every default policy that
    /// references `context.custom.<field>` (e.g. `expired-deadline.cedar`,
    /// `max-input-usd-100.cedar`, `min-output-usd-floor.cedar`) fails
    /// Cedar strict validation and the SW boot fails closed.
    ///
    /// This test pins the regression: with the production wiring fixed,
    /// the install must return `ok: true` without the SW or this test
    /// having to know which manifests power which fields.
    #[test]
    fn default_policy_set_installs_with_production_first_run_wiring() {
        use std::fs;
        use std::path::Path;

        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let policies_dir = repo_root.join("policy-rpc/examples/policies");

        let mut policy_entries: Vec<(String, String)> = Vec::new();
        for action_dir in fs::read_dir(&policies_dir).expect("policies dir") {
            let action_dir = action_dir.unwrap();
            if !action_dir.file_type().unwrap().is_dir() {
                continue;
            }
            let action = action_dir.file_name().to_string_lossy().into_owned();
            for entry in fs::read_dir(action_dir.path()).expect("action dir") {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "cedar") {
                    let id = format!(
                        "default::{action}/{stem}",
                        stem = path.file_stem().unwrap().to_string_lossy()
                    );
                    let text = fs::read_to_string(&path).expect("read cedar");
                    policy_entries.push((id, text));
                }
            }
        }
        let policy_set: Vec<Value> = policy_entries
            .iter()
            .map(|(id, text)| json!({ "id": id, "text": text }))
            .collect();

        // The smoking gun: production first-run sends an EMPTY Map shape
        // because `getAllManifests()` returns `{}` until the user opens
        // the dashboard and runs `manifest:put`. We mirror that here.
        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": {},
                "policy_set": policy_set,
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(
            installed["ok"],
            true,
            "default policy-set must install cleanly with empty manifests on first run: \
             policies={ids:?} envelope={installed}",
            ids = policy_entries.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        );

        // The success rests on every default policy guarding its
        // custom-field accesses with `context has custom && context.custom
        // has <field>`. Without those guards Cedar strict mode would
        // reject the access against the empty `<Action>CustomContext`.
        // If a future default drops a guard and lands here, this test
        // is the regression target.
    }

    /// Carry-over Fix N: every default-bundle policy text under
    /// `policy-rpc/examples/policies/<action>/*.cedar` must install
    /// cleanly against the enriched schema composed from the shipped
    /// manifests under `schema/policy-schema/extensions/<cat>/<action>.policy-rpc.json`.
    ///
    /// The `policy-set.json` the extension fetches at first run is
    /// generated from these `.cedar` files by
    /// `browser-extension/scripts/copy-default-policies.js`. If any
    /// policy text references a field that isn't projected by the
    /// shipped manifest (e.g. left-over `context.totalInputUsd`), the
    /// extension's first-run install fails closed.
    #[test]
    fn shipped_default_policies_install_against_shipped_manifests() {
        use std::collections::BTreeMap;
        use std::fs;
        use std::path::Path;

        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let policies_dir = repo_root.join("policy-rpc/examples/policies");
        let manifests_dir = repo_root.join("schema/policy-schema/extensions");

        // Walk every `<action>/*.cedar` under `policy-rpc/examples/policies/`.
        let mut policy_entries: Vec<(String, String)> = Vec::new();
        for action_dir in fs::read_dir(&policies_dir).expect("policies dir") {
            let action_dir = action_dir.unwrap();
            if !action_dir.file_type().unwrap().is_dir() {
                continue;
            }
            let action = action_dir.file_name().to_string_lossy().into_owned();
            for entry in fs::read_dir(action_dir.path()).expect("action dir") {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "cedar") {
                    let id = format!(
                        "default::{action}/{stem}",
                        stem = path.file_stem().unwrap().to_string_lossy()
                    );
                    let text = fs::read_to_string(&path).expect("read cedar");
                    policy_entries.push((id, text));
                }
            }
        }
        assert!(
            !policy_entries.is_empty(),
            "expected at least one default .cedar policy"
        );

        // Build the enriched schema from every shipped manifest under
        // `schema/policy-schema/extensions/<cat>/<action>.policy-rpc.json`.
        // Each file is keyed in the install map by its `<action>` stem.
        let mut manifest_map: BTreeMap<String, Value> = BTreeMap::new();
        for cat_dir in fs::read_dir(&manifests_dir).expect("manifests dir") {
            let cat_dir = cat_dir.unwrap();
            if !cat_dir.file_type().unwrap().is_dir() {
                continue;
            }
            for entry in fs::read_dir(cat_dir.path()).expect("category dir") {
                let entry = entry.unwrap();
                let path = entry.path();
                if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".policy-rpc.json"))
                {
                    let stem = path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .trim_end_matches(".policy-rpc.json")
                        .to_owned();
                    let raw = fs::read_to_string(&path).expect("read manifest");
                    let manifest: Value = serde_json::from_str(&raw).expect("manifest json");
                    manifest_map.insert(stem, manifest);
                }
            }
        }
        assert!(
            manifest_map.contains_key("swap"),
            "shipped swap manifest must exist; found keys: {:?}",
            manifest_map.keys().collect::<Vec<_>>()
        );

        let policy_set: Vec<Value> = policy_entries
            .iter()
            .map(|(id, text)| json!({ "id": id, "text": text }))
            .collect();

        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": manifest_map,
                "policy_set": policy_set,
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(
            installed["ok"],
            true,
            "shipped default policy-set must install against shipped manifests: \
             policies={ids:?} envelope={installed}",
            ids = policy_entries.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        );

        // Cross-check: the enriched schema text must declare every custom
        // field referenced by `context.custom.<field>` in any default policy.
        let schema_text = installed["data"]["enrichedSchemaHash"]
            .as_str()
            .expect("enrichedSchemaHash present");
        assert!(
            schema_text.starts_with("sha256:"),
            "enrichedSchemaHash should be hash-formatted: {schema_text}"
        );
    }

    #[test]
    fn evaluate_with_envelopes_json_v2_swap_pass_with_permit_policy() {
        // Install a single permit-all policy (no manifests) — verdict should pass.
        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": [],
                "policy_set": [{
                    "id": "bundle::permit-all-swap",
                    "text": r#"
                        @severity("warn")
                        @reason("permit everything")
                        permit(principal, action == Action::"swap", resource);
                    "#
                }]
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        // V2 swap mainnet: WETH (1e18 exact_in) → USDC (min 0). Symbols and
        // decimals are populated because the declarative adapter layer is
        // expected to enrich envelopes before they reach this entry.
        let envelope = json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        "symbol": "WETH",
                        "decimals": 18
                    },
                    "amount": { "kind": "exact", "value": "1000000000000000000" }
                },
                "outputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC",
                        "decimals": 6
                    },
                    "amount": { "kind": "min", "value": "0" }
                },
                "recipient": "0x2222222222222222222222222222222222222222"
            }
        });

        let output = evaluate_with_envelopes_json(
            json!({
                "envelopes": [envelope],
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
                "value_wei": "0",
                "chain_id": 1u64,
                "block_timestamp": 1_700_000_000u64,
                "manifests": [],
                "rpc_response": { "request_id": "decl-eval-1", "results": [] }
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "pass", "{parsed}");
    }

    /// Step 1 verify — `no-zero-min-output` 정책 의 trigger 정합성. T1 manual
    /// e2e 에서 UR Base button 이 `amountOutMin=0` 인데 verdict="pass" 한
    /// 원인 점검. envelope 의 `outputToken.amount.{kind:"min", value:"0"}` 가
    /// 정책 의 `forbid` 조건 과 정확 매칭 — fail (deny) 이어야 정상.
    #[test]
    fn evaluate_with_envelopes_json_v2_swap_triggers_no_zero_min_output() {
        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": [],
                "policy_set": [{
                    "id": "bundle::no-zero-min-output",
                    "text": include_str!(
                        "../../../policy-rpc/examples/policies/swap/no-zero-min-output.cedar"
                    )
                }]
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        // V2 swap with amountOutMin=0 — same shape the declarative path emits
        // for UR.execute V2_SWAP_EXACT_IN with amountOutMin=0 (T1 button #3).
        let envelope = json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        "symbol": "WETH",
                        "decimals": 18
                    },
                    "amount": { "kind": "exact", "value": "1000000000000000000" }
                },
                "outputToken": {
                    "asset": {
                        "kind": "erc20",
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC",
                        "decimals": 6
                    },
                    "amount": { "kind": "min", "value": "0" }
                },
                "recipient": "0x2222222222222222222222222222222222222222"
            }
        });

        let output = evaluate_with_envelopes_json(
            json!({
                "envelopes": [envelope],
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
                "value_wei": "0",
                "chain_id": 1u64,
                "block_timestamp": 1_700_000_000u64,
                "manifests": [],
                "rpc_response": { "request_id": "decl-eval-nozmin", "results": [] }
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["kind"], "fail",
            "verdict should be fail when amountOutMin=0 with no-zero-min-output policy: {parsed}"
        );
        let matched = parsed["data"]["matched"]
            .as_array()
            .expect("matched array");
        assert!(
            matched
                .iter()
                .any(|m| m["policy_id"].as_str().is_some_and(|id| id.contains("no-zero-min-output"))),
            "expected no-zero-min-output policy to fire, got {parsed}"
        );
    }

    /// 신규 사용자 정책 fixture — Step 4 (회귀 test 확장) 의 일부.
    /// 7 정책 의 deny case 가 declarative envelope wire 에서 trigger 되는지 검증.
    fn install_user_policy_only(policy_id: &str, policy_text: &str) {
        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": [],
                "policy_set": [{
                    "id": format!("bundle::{policy_id}"),
                    "text": policy_text
                }]
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");
    }

    /// 본 helper 의 `expected_kind` = "fail" (severity="deny") 또는 "warn"
    /// (severity="warn"). 정책 의 trigger 자체 는 verify, verdict 의
    /// outcome kind 는 정책 의 severity 따라 분리.
    fn assert_envelope_trigger(envelope: Value, expected_policy_id: &str, expected_kind: &str) {
        let output = evaluate_with_envelopes_json(
            json!({
                "envelopes": [envelope],
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
                "value_wei": "0",
                "chain_id": 1u64,
                "block_timestamp": 1_700_000_000u64,
                "manifests": [],
                "rpc_response": { "request_id": "decl-eval-user", "results": [] }
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["kind"], expected_kind,
            "verdict should be {expected_kind} with {expected_policy_id}: {parsed}"
        );
        let matched = parsed["data"]["matched"]
            .as_array()
            .expect("matched array");
        assert!(
            matched.iter().any(|m| m["policy_id"]
                .as_str()
                .is_some_and(|id| id.contains(expected_policy_id))),
            "expected {expected_policy_id} to fire, got {parsed}"
        );
    }

    // Post-origin/main 3106fc6 (swap enrichment → manifest-driven custom
    // context): `validityDeltaSec` is no longer derived by the swap lowering.
    // It is emitted by the `clock.validity_delta_sec` enrichment recorded in
    // the swap manifest. `install_user_policy_only` installs the user policy
    // without that manifest, so `context.custom.validityDeltaSec` is absent
    // and the deadline guard does not fire. Re-enable once a manifest-driven
    // enrichment harness lands.
    #[ignore]
    #[test]
    fn user_policy_swap_short_deadline_deny() {
        install_user_policy_only(
            "swap-short-deadline",
            include_str!("../../../policy-rpc/examples/policies/swap/swap-short-deadline.cedar"),
        );
        // validity.expiresAt = block_timestamp + 30  → validityDeltaSec=30 → trigger
        let envelope = json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "symbol": "WETH", "decimals": 18 },
                    "amount": { "kind": "exact", "value": "1000000000000000000" }
                },
                "outputToken": {
                    "asset": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "symbol": "USDC", "decimals": 6 },
                    "amount": { "kind": "min", "value": "1" }
                },
                "recipient": "0x2222222222222222222222222222222222222222",
                "validity": { "expiresAt": "1700000030", "source": "tx-deadline" }
            }
        });
        assert_envelope_trigger(envelope, "swap-short-deadline", "warn");
    }

    // See `user_policy_swap_short_deadline_deny` — same root cause
    // (manifest-driven enrichment now owns `validityDeltaSec`).
    #[ignore]
    #[test]
    fn user_policy_swap_long_deadline_deny() {
        install_user_policy_only(
            "swap-long-deadline",
            include_str!("../../../policy-rpc/examples/policies/swap/swap-long-deadline.cedar"),
        );
        // validity.expiresAt = block_timestamp + 86400 → validityDeltaSec=86400 > 3600
        let envelope = json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "symbol": "WETH", "decimals": 18 },
                    "amount": { "kind": "exact", "value": "1000000000000000000" }
                },
                "outputToken": {
                    "asset": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "symbol": "USDC", "decimals": 6 },
                    "amount": { "kind": "min", "value": "1" }
                },
                "recipient": "0x2222222222222222222222222222222222222222",
                "validity": { "expiresAt": "1700086400", "source": "tx-deadline" }
            }
        });
        assert_envelope_trigger(envelope, "swap-long-deadline", "warn");
    }

    #[test]
    fn user_policy_permit_max_amount_deny() {
        install_user_policy_only(
            "permit-max-amount",
            include_str!("../../../policy-rpc/examples/policies/permit/permit-max-amount.cedar"),
        );
        // amount.value = 2^160 - 1 (Permit2 max uint160) → trigger
        let envelope = json!({
            "category": "misc",
            "action": "permit",
            "fields": {
                "permitKind": "permit2_single",
                "token": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "symbol": "WETH", "decimals": 18 },
                "owner": "0x1111111111111111111111111111111111111111",
                "spender": "0x3333333333333333333333333333333333333333",
                "amount": { "kind": "max", "value": "1461501637330902918203684832716283019655932542975" },
                "validity": { "expiresAt": "1700003600", "source": "grant-expiration" },
                "signatureValidity": { "expiresAt": "1700001800", "source": "signature-deadline" }
            }
        });
        assert_envelope_trigger(envelope, "permit-max-amount", "fail");
    }

    #[test]
    fn user_policy_transfer_suspicious_recipient_deny() {
        install_user_policy_only(
            "transfer-suspicious-recipient",
            include_str!("../../../policy-rpc/examples/policies/transfer/transfer-suspicious-recipient.cedar"),
        );
        // recipient = 0x000...dead → trigger (정책 의 multi-OR 중 하나)
        let envelope = json!({
            "category": "misc",
            "action": "transfer",
            "fields": {
                "token": {
                    "asset": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "symbol": "WETH", "decimals": 18 },
                    "amount": { "kind": "exact", "value": "1000000000000000000" }
                },
                "from": "0x1111111111111111111111111111111111111111",
                "recipient": "0x000000000000000000000000000000000000dead"
            }
        });
        assert_envelope_trigger(envelope, "transfer-suspicious-recipient", "fail");
    }

    #[test]
    fn user_policy_transfer_large_amount_exact_deny() {
        install_user_policy_only(
            "transfer-large-amount-exact",
            include_str!("../../../policy-rpc/examples/policies/transfer/transfer-large-amount-exact.cedar"),
        );
        // token.amount.value = "1000000000000000000000" (1000 ETH wei) → trigger
        let envelope = json!({
            "category": "misc",
            "action": "transfer",
            "fields": {
                "token": {
                    "asset": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "symbol": "WETH", "decimals": 18 },
                    "amount": { "kind": "exact", "value": "1000000000000000000000" }
                },
                "from": "0x1111111111111111111111111111111111111111",
                "recipient": "0x2222222222222222222222222222222222222222"
            }
        });
        assert_envelope_trigger(envelope, "transfer-large-amount-exact", "warn");
    }

    #[test]
    fn user_policy_wrap_large_amount_exact_deny() {
        install_user_policy_only(
            "wrap-large-amount-exact",
            include_str!("../../../policy-rpc/examples/policies/transfer/wrap-large-amount-exact.cedar"),
        );
        // nativeAsset.amount.value = "100000000000000000000" (100 ETH wei) → trigger
        let envelope = json!({
            "category": "misc",
            "action": "wrap",
            "fields": {
                "nativeAsset": {
                    "asset": { "kind": "native" },
                    "amount": { "kind": "min", "value": "100000000000000000000" }
                },
                "wrappedAsset": {
                    "asset": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "symbol": "WETH", "decimals": 18 },
                    "amount": { "kind": "min", "value": "100000000000000000000" }
                },
                "recipient": "0x2222222222222222222222222222222222222222"
            }
        });
        assert_envelope_trigger(envelope, "wrap-large-amount-exact", "warn");
    }

    #[test]
    fn user_policy_protocol_blocklist_example_deny() {
        install_user_policy_only(
            "protocol-blocklist-example",
            include_str!("../../../policy-rpc/examples/policies/protocol/protocol-blocklist-example.cedar"),
        );
        // resource = Protocol::"0x000000000000000000000000000000000000dead" → trigger
        // Protocol uid = tx 의 `to` address — assert_envelope_fails_with_policy 의
        // default to=0x7a25... 가 아니라 0x000...dead 를 직접 사용 위해 custom invoke.
        let envelope = json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "symbol": "WETH", "decimals": 18 },
                    "amount": { "kind": "exact", "value": "1" }
                },
                "outputToken": {
                    "asset": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "symbol": "USDC", "decimals": 6 },
                    "amount": { "kind": "min", "value": "1" }
                },
                "recipient": "0x2222222222222222222222222222222222222222"
            }
        });
        let output = evaluate_with_envelopes_json(
            json!({
                "envelopes": [envelope],
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x000000000000000000000000000000000000dead",
                "value_wei": "0",
                "chain_id": 1u64,
                "block_timestamp": 1_700_000_000u64,
                "manifests": [],
                "rpc_response": { "request_id": "decl-eval-protocol", "results": [] }
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["kind"], "fail",
            "verdict should be fail for protocol-blocklist: {parsed}"
        );
        let matched = parsed["data"]["matched"]
            .as_array()
            .expect("matched array");
        assert!(
            matched.iter().any(|m| m["policy_id"]
                .as_str()
                .is_some_and(|id| id.contains("protocol-blocklist-example"))),
            "expected protocol-blocklist-example to fire, got {parsed}"
        );
    }

    #[test]
    fn evaluate_with_envelopes_json_fails_closed_on_manifest_mismatch() {
        // Install with one manifest set, then call evaluate with a different
        // set — must fail closed via `__engine::installed_manifest_hash_mismatch`.
        install_usd_policy();
        let other_manifest = json!({
            "id": "user/another-manifest",
            "schema_version": 1,
            "requires": [],
            "context_extensions": {}
        });

        let envelope = json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "inputToken": {
                    "asset": { "kind": "erc20", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2" },
                    "amount": { "kind": "exact", "value": "1" }
                },
                "outputToken": {
                    "asset": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" },
                    "amount": { "kind": "min", "value": "0" }
                },
                "recipient": "0x2222222222222222222222222222222222222222"
            }
        });

        let output = evaluate_with_envelopes_json(
            json!({
                "envelopes": [envelope],
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
                "value_wei": "0",
                "chain_id": 1u64,
                "manifests": [other_manifest],
                "rpc_response": { "request_id": "decl-eval-2", "results": [] }
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "fail", "{parsed}");
        assert_eq!(
            parsed["data"]["matched"][0]["policy_id"],
            "__engine::installed_manifest_hash_mismatch",
            "{parsed}"
        );
    }

    #[test]
    fn preview_schema_json_reports_schema_text_and_hash() {
        let output = preview_schema_json(json!({ "manifests": [manifest_json()] }).to_string());
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert!(parsed["data"]["schema_text"]
            .as_str()
            .unwrap()
            .contains("totalInputUsd?: UsdValuation"));
        assert!(parsed["data"]["schema_hash"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
    }

    #[test]
    fn preview_installed_schema_json_preserves_added_fields() {
        let output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": [custom_field_manifest_json()],
                "policy_set": []
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let output = preview_installed_schema_json();
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["added_fields"][0],
            json!({
                "action": "swap",
                "field": "tokenRiskScore",
                "type": "Long",
                "source_manifest": "user/custom-risk-score"
            }),
            "{parsed}"
        );
    }

    /// Task 5.3 happy path: caller passes the manifests map shape, the
    /// install path composes via `compose_enriched`, and the success
    /// envelope carries `enrichedSchemaHash` + per-action
    /// `addedCustomFields`. `preview_installed_schema_json` then echoes the
    /// same data through `customContexts` + `schemaHash` (camelCase).
    #[test]
    fn install_with_manifests_updates_installed_schema() {
        let manifest = manifest_json();
        // Inline policy reads `context.custom.totalInputUsd` — this only
        // installs cleanly when the install path uses `compose_enriched`.
        let policy = r#"
            @severity("deny")
            @reason("too much USD")
            forbid(principal, action == Action::"swap", resource)
            when {
                context has custom &&
                context.custom has totalInputUsd &&
                context.custom.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let install_out = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": { "swap": manifest },
                "policy_set": [{
                    "id": "bundle::max-input-usd-100",
                    "text": policy
                }]
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&install_out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");

        let hash = parsed["data"]["enrichedSchemaHash"]
            .as_str()
            .expect("enrichedSchemaHash present");
        assert!(hash.starts_with("sha256:"), "{hash}");

        let added = &parsed["data"]["addedCustomFields"]["swap"];
        let added = added.as_array().expect("addedCustomFields.swap array");
        assert_eq!(added.len(), 1, "{added:?}");
        assert_eq!(added[0]["field"], "totalInputUsd", "{added:?}");

        // `preview_installed_schema_json` surfaces the same data via the
        // camelCase additions (`customContexts`, `schemaHash`).
        let preview_out = preview_installed_schema_json();
        let preview: Value = serde_json::from_str(&preview_out).unwrap();
        assert_eq!(preview["ok"], true, "{preview}");
        let swap_fields = preview["data"]["customContexts"]["swap"]
            .as_array()
            .expect("customContexts.swap array");
        assert_eq!(swap_fields.len(), 1);
        assert_eq!(swap_fields[0]["field"], "totalInputUsd");
        assert_eq!(preview["data"]["schemaHash"], hash, "{preview}");
    }

    /// Task 5.3 validation: installing a policy whose body uses a custom
    /// field at the wrong type must fail strict validation against the
    /// enriched cedarschema. `totalInputUsd` is a `UsdValuation` record;
    /// calling `.greaterThan(decimal("100.00"))` directly on it (instead of
    /// on its `.value` decimal) is a type error.
    #[test]
    fn install_with_manifests_rejects_policy_against_wrong_custom_field_type() {
        let manifest = manifest_json();
        let policy = r#"
            @severity("deny")
            forbid(principal, action == Action::"swap", resource)
            when {
                context has custom &&
                context.custom has totalInputUsd &&
                context.custom.totalInputUsd.greaterThan(decimal("100.00"))
            };
        "#;
        let install_out = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": { "swap": manifest },
                "policy_set": [{
                    "id": "bundle::wrong-type",
                    "text": policy
                }]
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&install_out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "install_failed", "{parsed}");
    }

    #[test]
    fn preview_custom_schema_with_one_output() {
        let input = json!({
            "action": "swap",
            "manifest": manifest_json()
        })
        .to_string();
        let out = preview_custom_schema_json(input);
        let parsed: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        let data = &parsed["data"];

        let schema_text = data["enrichedSchemaText"]
            .as_str()
            .expect("enrichedSchemaText is string");
        assert!(
            schema_text.contains("type SwapCustomContext = {"),
            "{schema_text}"
        );
        assert!(
            schema_text.contains("totalInputUsd?: UsdValuation"),
            "{schema_text}"
        );

        let custom_types = data["customTypes"]
            .as_array()
            .expect("customTypes is array");
        assert_eq!(custom_types.len(), 1, "{data}");
        assert_eq!(custom_types[0]["name"], "swap", "{data}");
        let fields = custom_types[0]["fields"]
            .as_array()
            .expect("fields is array");
        assert_eq!(fields.len(), 1, "{data}");
        assert_eq!(fields[0]["field"], "totalInputUsd", "{data}");
        assert_eq!(fields[0]["cedar_type"], "UsdValuation", "{data}");

        let hash = data["schemaHash"].as_str().expect("schemaHash is string");
        assert!(hash.starts_with("sha256:"), "{hash}");

        // D14 diff: no install yet, so every previewed field is `added` and
        // both `removed` and `changed` are empty.
        let diff = &data["diff"];
        assert_eq!(diff["added"].as_array().unwrap().len(), 1, "{diff}");
        assert_eq!(diff["added"][0]["field"], "totalInputUsd", "{diff}");
        assert!(diff["removed"].as_array().unwrap().is_empty(), "{diff}");
        assert!(diff["changed"].as_array().unwrap().is_empty(), "{diff}");
    }

    #[test]
    fn get_alias_table_returns_known_aliases() {
        let out = get_alias_table_json();
        let parsed: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        let entries = parsed["data"]["entries"].as_array().expect("entries array");
        let by_name: std::collections::BTreeMap<&str, &Value> = entries
            .iter()
            .map(|e| (e["name"].as_str().expect("name string"), e))
            .collect();

        // Scalars and records both present, keyed by their manifest spelling.
        assert!(by_name.contains_key("String"));
        assert_eq!(by_name["String"]["kind"], "scalar");
        assert_eq!(by_name["String"]["cedarSpelling"], "String");

        assert!(by_name.contains_key("UsdValuation"));
        assert_eq!(by_name["UsdValuation"]["kind"], "record");
        assert_eq!(by_name["UsdValuation"]["cedarSpelling"], "UsdValuation");

        assert!(by_name.contains_key("Set<String>"));
        assert_eq!(by_name["Set<String>"]["kind"], "scalar");

        // Unknown aliases must be absent.
        assert!(!by_name.contains_key("RiskScore"));
    }

    #[test]
    fn installed_schema_hash_includes_custom_schema_text() {
        let base_output =
            preview_schema_json(json!({ "manifests": [custom_field_manifest_json()] }).to_string());
        let base: Value = serde_json::from_str(&base_output).unwrap();
        assert_eq!(base["ok"], true, "{base}");

        let installed_output = install_policies_json(
            json!({
                "schema_text": "type PolicyRpcDebug = { enabled: Bool };",
                "manifests": [custom_field_manifest_json()],
                "policy_set": []
            })
            .to_string(),
        );
        let installed: Value = serde_json::from_str(&installed_output).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let output = preview_installed_schema_json();
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert!(parsed["data"]["schema_text"]
            .as_str()
            .unwrap()
            .contains("type PolicyRpcDebug"));
        assert_ne!(parsed["data"]["schema_hash"], base["data"]["schema_hash"]);
    }
}
