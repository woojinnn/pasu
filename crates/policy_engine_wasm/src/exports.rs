//! Thin `#[wasm_bindgen]` JSON-string exports.

use crate::dto::{
    EngineErrorDto, Envelope, HostFactPlanDto, HostSnapshotDto, InstallPoliciesInputDto,
    MatchedPolicyDto, OracleEntryDto, VerdictDto, WindowKeyPlanDto,
};
use crate::state::{
    registry, signature_registry, snapshot_oracle_from_entries, EngineState, FixedClock,
    SnapshotApprovals, SnapshotPortfolio, SnapshotStatWindows, STATE,
};
use policy_engine::core::{Action, Request};
use policy_engine::host::oracle::SnapshotOracle;
use policy_engine::host::HostCapabilities;
use policy_engine::lowering::{required_host_facts, required_window_keys};
use policy_engine::pipeline::PipelineError;
use policy_engine::policy::{
    MatchedPolicy, PolicyEngineBuilder, PolicyRequestOrigin, Severity, Verdict,
};
use policy_engine::Pipeline;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn install_policies_json(policies_json: String) -> String {
    let result = (|| -> Result<(), EngineErrorDto> {
        let input: InstallPoliciesInputDto =
            serde_json::from_str(&policies_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        let mut builder = PolicyEngineBuilder::new();
        if !input.schema_text.trim().is_empty() {
            builder = builder.add_schema_text(input.schema_text);
        }
        for policy in input.policy_set {
            builder = builder.add_text(namespace_policy_text(&policy.id, &policy.text));
        }

        let policies = builder
            .build()
            .map_err(|error| EngineErrorDto::new("install_failed", error.to_string()))?;

        STATE.with(|state| {
            *state.borrow_mut() = Some(EngineState { policies });
        });
        Ok(())
    })();

    match result {
        Ok(()) => Envelope::ok(()).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

#[wasm_bindgen]
pub fn build_action_for_request_json(request_json: String) -> String {
    let result = (|| -> Result<serde_json::Value, EngineErrorDto> {
        let request = parse_request(&request_json)?;
        let registry = registry();
        let signature_registry = signature_registry();
        let oracle = SnapshotOracle::new();

        let action = STATE.with(|state| {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(not_installed_error)?;
            let host = HostCapabilities::new(&oracle);
            Pipeline::new(&registry, host, &state.policies)
                .with_signature_registry(&signature_registry)
                .build_action_for(&request)
                .map_err(pipeline_error)
        })?;

        serde_json::to_value(action)
            .map_err(|error| EngineErrorDto::new("serialize_action", error.to_string()))
    })();

    match result {
        Ok(action) => Envelope::ok(action).to_json(),
        Err(error) => Envelope::<serde_json::Value>::err(error.kind, error.message).to_json(),
    }
}

#[wasm_bindgen]
pub fn tier1_fact_plan_json(action_json: String) -> String {
    let result = (|| -> Result<HostFactPlanDto, EngineErrorDto> {
        let action = parse_action(&action_json)?;
        Ok(required_host_facts(&action).into())
    })();

    match result {
        Ok(plan) => Envelope::ok(plan).to_json(),
        Err(error) => Envelope::<HostFactPlanDto>::err(error.kind, error.message).to_json(),
    }
}

#[wasm_bindgen]
pub fn tier2_window_keys_json(action_json: String, oracle_snapshot_json: String) -> String {
    let result = (|| -> Result<WindowKeyPlanDto, EngineErrorDto> {
        let action = parse_action(&action_json)?;
        let entries: Vec<OracleEntryDto> =
            serde_json::from_str(&oracle_snapshot_json).map_err(|error| {
                EngineErrorDto::new("invalid_oracle_snapshot_json", error.to_string())
            })?;
        let oracle = snapshot_oracle_from_entries(&entries);
        Ok(required_window_keys(&action, &oracle).into())
    })();

    match result {
        Ok(plan) => Envelope::ok(plan).to_json(),
        Err(error) => Envelope::<WindowKeyPlanDto>::err(error.kind, error.message).to_json(),
    }
}

#[wasm_bindgen]
pub fn evaluate_json(request_json: String, host_snapshot_json: String) -> String {
    let verdict = (|| -> Result<Verdict, EngineErrorDto> {
        let request = parse_request(&request_json)?;
        let snapshot: HostSnapshotDto =
            serde_json::from_str(&host_snapshot_json).map_err(|error| {
                EngineErrorDto::new("invalid_host_snapshot_json", error.to_string())
            })?;

        let oracle = snapshot_oracle_from_entries(&snapshot.oracle);
        let portfolio = SnapshotPortfolio::from_entries(&snapshot.balances);
        let approvals = SnapshotApprovals::from_entries(&snapshot.allowances);
        let stats = SnapshotStatWindows::from_entries(&snapshot.windows);
        let clock = FixedClock(snapshot.now_ts.unwrap_or(0));
        let registry = registry();
        let signature_registry = signature_registry();

        STATE.with(|state| {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(not_installed_error)?;
            let host = HostCapabilities::new(&oracle)
                .with_clock(&clock)
                .with_portfolio(&portfolio)
                .with_approvals(&approvals)
                .with_stats(&stats);
            Pipeline::new(&registry, host, &state.policies)
                .with_signature_registry(&signature_registry)
                .evaluate(&request)
                .map_err(pipeline_error)
        })
    })();

    let dto = match verdict {
        Ok(verdict) => verdict_to_dto(verdict),
        Err(error) => engine_error_verdict(error),
    };
    Envelope::ok(dto).to_json()
}

fn parse_request(request_json: &str) -> Result<Request, EngineErrorDto> {
    serde_json::from_str(request_json)
        .map_err(|error| EngineErrorDto::new("invalid_request_json", error.to_string()))
}

fn parse_action(action_json: &str) -> Result<Action, EngineErrorDto> {
    serde_json::from_str(action_json)
        .map_err(|error| EngineErrorDto::new("invalid_action_json", error.to_string()))
}

fn not_installed_error() -> EngineErrorDto {
    EngineErrorDto::new(
        "not_installed",
        "install_policies_json must be called first",
    )
}

fn pipeline_error(error: PipelineError) -> EngineErrorDto {
    EngineErrorDto::new(pipeline_error_kind(&error), error.to_string())
}

fn pipeline_error_kind(error: &PipelineError) -> &'static str {
    match error {
        PipelineError::Ambiguous(_) => "adapter_ambiguous",
        PipelineError::AdapterBuild(_) => "adapter_build",
        PipelineError::Lowering(_) => "lowering_rejected",
        PipelineError::Policy(_) => "policy",
    }
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
    use serde_json::{json, Value};

    fn tx_request_json() -> Value {
        json!({
            "Tx": {
                "chain_id": 1,
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x2222222222222222222222222222222222222222",
                "value_wei": "0",
                "data": [0xde, 0xad, 0xbe, 0xef],
                "gas": null,
                "nonce": null
            }
        })
    }

    fn empty_snapshot_json() -> Value {
        json!({
            "oracle": [],
            "balances": [],
            "allowances": [],
            "now_ts": 1_700_000_000_u64,
            "windows": []
        })
    }

    fn install_empty_policies() {
        let output = install_policies_json(
            json!({
                "schema_text": "",
                "policy_set": []
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
    }

    fn dex_action_json() -> Value {
        json!({
            "dex": {
                "actor": "0x1111111111111111111111111111111111111111",
                "target": "0xe592427a0aece92de3edee1f18e0157c05861564",
                "value_wei": "0",
                "facts": {
                    "protocol_ids": ["uniswap_v3"],
                    "input_tokens": [{
                        "chain_id": 1,
                        "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        "symbol": "WETH",
                        "decimals": 18,
                        "is_native": false
                    }],
                    "output_tokens": [{
                        "chain_id": 1,
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC",
                        "decimals": 6,
                        "is_native": false
                    }],
                    "total_input_usd": null,
                    "total_min_output_usd": null,
                    "max_fee_bps": null,
                    "has_zero_min_output": false,
                    "has_external_recipient": false,
                    "total_input_fraction_of_portfolio_bps": null,
                    "allowances_cover_inputs": null,
                    "window_stats": null
                },
                "oracle_requirements": [],
                "trace": {"steps": []}
            }
        })
    }

    #[test]
    fn install_round_trip_injects_id_before_evaluation() {
        let policy = r#"
            @severity("deny")
            @reason("blocked")
            forbid(principal, action == Action::"other", resource);
        "#;
        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "policy_set": [{
                    "id": "bundle::block-other",
                    "text": policy
                }]
            })
            .to_string(),
        );
        let install: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(install["ok"], true, "{install}");

        let output = evaluate_json(
            tx_request_json().to_string(),
            empty_snapshot_json().to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "fail", "{parsed}");
        assert_eq!(
            parsed["data"]["matched"][0]["policy_id"], "bundle::block-other",
            "{parsed}"
        );
    }

    #[test]
    fn install_overrides_existing_single_id_with_entry_id() {
        let policy = r#"
            @id("evil/untrusted")
            @severity("deny")
            @reason("blocked")
            forbid(principal, action == Action::"other", resource);
        "#;
        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "policy_set": [{
                    "id": "bundle::block-other",
                    "text": policy
                }]
            })
            .to_string(),
        );
        let install: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(install["ok"], true, "{install}");

        let output = evaluate_json(
            tx_request_json().to_string(),
            empty_snapshot_json().to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "fail", "{parsed}");
        assert_eq!(
            parsed["data"]["matched"][0]["policy_id"], "bundle::block-other",
            "{parsed}"
        );
    }

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

    #[test]
    fn build_action_returns_other() {
        install_empty_policies();

        let output = build_action_for_request_json(tx_request_json().to_string());
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert!(parsed["data"].get("other").is_some(), "{parsed}");
        assert_eq!(parsed["data"]["other"]["selector"], "0xdeadbeef");
    }

    #[test]
    fn tier1_for_dex_returns_expected_host_facts() {
        let output = tier1_fact_plan_json(dex_action_json().to_string());
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["tokens_for_oracle"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(parsed["data"]["balances"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["data"]["allowances"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["data"]["clock_required"], false);
    }

    #[test]
    fn tier2_emits_stat_key_wire_strings() {
        let output = tier2_window_keys_json(dex_action_json().to_string(), "[]".to_string());
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        let names: Vec<&str> = parsed["data"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"swapVolumeUsd24h"), "{parsed}");
        assert!(names.contains(&"swapCount24h"), "{parsed}");
    }

    #[test]
    fn evaluate_pass_on_unknown_calldata() {
        install_empty_policies();

        let output = evaluate_json(
            tx_request_json().to_string(),
            empty_snapshot_json().to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "pass", "{parsed}");
    }
}
