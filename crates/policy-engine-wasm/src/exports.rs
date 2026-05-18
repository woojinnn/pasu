//! Thin `#[wasm_bindgen]` JSON-string exports.

use crate::dto::{
    EngineErrorDto, Envelope, EvaluatePolicyRpcInputDto, InstallPoliciesInputDto, MatchedPolicyDto,
    PlanPolicyRpcInputDto, PolicyRpcPlanDto, PreviewSchemaInputDto, RawRequestDto, VerdictDto,
};
use alloy_primitives::U256;
use policy_engine::lowering::policy_request_from_envelope;
use policy_engine::policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyRequestOrigin, Severity, Verdict,
};
use policy_engine::policy_rpc::{
    apply_rpc_results_with_indices, manifest_set_hash, plan_calls, system_fail_verdict, RootInput,
};
use policy_engine::schema::AddedContextField;
use policy_engine::schema::{schema_hash, PolicySchemaComposer};
use policy_engine::{ActionAddress, ActionEnvelope, DecimalString};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

pub struct EngineState {
    pub policies: PolicyEngine,
    pub manifest_set_hash: String,
    pub schema_hash: String,
    pub schema_text: String,
    pub added_fields: Vec<AddedContextField>,
}

thread_local! {
    static STATE: RefCell<Option<EngineState>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn install_policies_json(policies_json: String) -> String {
    let result = (|| -> Result<(), EngineErrorDto> {
        let input: InstallPoliciesInputDto =
            serde_json::from_str(&policies_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        let schema_preview = PolicySchemaComposer::new()
            .with_manifests(&input.manifests)
            .map_err(|error| EngineErrorDto::new("schema_failed", error.to_string()))?
            .preview();
        let schema_text = if input.schema_text.trim().is_empty() {
            schema_preview.schema_text.clone()
        } else {
            format!("{}\n{}", schema_preview.schema_text, input.schema_text)
        };
        let installed_schema_hash = schema_hash(&schema_text);
        let mut builder = PolicyEngineBuilder::with_schema_text(schema_text.clone());
        for policy in input.policy_set {
            builder = builder.add_text(namespace_policy_text(&policy.id, &policy.text));
        }

        let policies = builder
            .build()
            .map_err(|error| EngineErrorDto::new("install_failed", error.to_string()))?;

        STATE.with(|state| {
            *state.borrow_mut() = Some(EngineState {
                policies,
                manifest_set_hash: manifest_set_hash(&input.manifests),
                schema_hash: installed_schema_hash,
                schema_text,
                added_fields: schema_preview.added_fields,
            });
        });
        Ok(())
    })();

    match result {
        Ok(()) => Envelope::ok(()).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

fn not_installed_error() -> EngineErrorDto {
    EngineErrorDto::new(
        "not_installed",
        "install_policies_json must be called first",
    )
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
    let reason = format!("__engine::{}", error.kind);
    VerdictDto::Fail {
        matched: vec![MatchedPolicyDto {
            policy_id: reason.clone(),
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
pub fn preview_schema_json(input_json: String) -> String {
    let result = (|| -> Result<policy_engine::schema::SchemaPreview, EngineErrorDto> {
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
    let result: Result<policy_engine::schema::SchemaPreview, EngineErrorDto> =
        STATE.with(|state| {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(not_installed_error)?;
            Ok(policy_engine::schema::SchemaPreview {
                schema_text: state.schema_text.clone(),
                schema_hash: state.schema_hash.clone(),
                added_fields: state.added_fields.clone(),
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

    fn install_usd_policy() {
        let output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": [manifest_json()],
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
    #[ignore = "TODO(phase-5.3): un-ignore once `install_policies_json` accepts the `manifests` map and composes via `compose_enriched` so the inline policy body's `context.custom.totalInputUsd` resolves against `SwapCustomContext`."]
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
        assert_eq!(
            parsed["data"]["matched"][0]["origin"], "action",
            "{parsed}"
        );
    }

    #[test]
    #[ignore = "TODO(phase-5/D11): policy-rpc/examples/policies/swap/max-input-usd-100.cedar and min-output-usd-floor.cedar now read context.custom.X but their matching .policy-rpc.json manifests still place outputs at top-level context. Re-enable once the materializer writes outputs under context.custom (Phase 5) and the example manifests retire their legacy context_extensions block."]
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
        let manifests = vec![max_manifest, min_manifest];

        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "manifests": manifests.clone(),
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
