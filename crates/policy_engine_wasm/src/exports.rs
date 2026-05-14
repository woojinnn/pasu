//! Thin `#[wasm_bindgen]` JSON-string exports.

use crate::dto::{
    AllowanceEntryDto, BalanceEntryDto, EngineErrorDto, Envelope, EvaluateEnvelopeInputDto,
    HostSnapshotDto, InstallPoliciesInputDto, MatchedPolicyDto, OracleEntryDto, VerdictDto,
};
use alloy_primitives::U256;
use policy_engine::core::{Address as CoreAddress, Token, UsdValuation as CoreUsdValuation};
use policy_engine::enrichment::enrich_envelope;
use policy_engine::host::oracle::SnapshotOracle;
use policy_engine::host::{HostCapabilities, MockApprovals, MockPortfolio};
use policy_engine::lowering::policy_request_from_envelope;
use policy_engine::policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyRequestOrigin, Severity, Verdict,
};
use policy_engine::{ActionAddress, ActionEnvelope, DecimalString};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

pub struct EngineState {
    pub policies: PolicyEngine,
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
    use serde_json::{json, Value};

    fn empty_snapshot_json() -> Value {
        json!({
            "oracle": [],
            "balances": [],
            "allowances": [],
            "now_ts": 1_700_000_000_u64,
            "windows": []
        })
    }

    // The bundled Cedar schema (composed by `PolicyEngineBuilder::new()`) already
    // contains the `swap` action declaration, so adding `swap.cedarschema` again
    // would fail with a duplicate-declaration error. Tests that want a swap-aware
    // engine therefore install with empty `schema_text`.
    fn install_empty_policies_with_swap_schema() {
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

    fn asset_ref(address: &str, symbol: &str, decimals: u64) -> Value {
        json!({
            "kind": "erc20",
            "address": address,
            "symbol": symbol,
            "decimals": decimals
        })
    }

    fn amount_constraint(kind: &str, value: &str) -> Value {
        json!({
            "kind": kind,
            "value": value
        })
    }

    fn approve_envelope_json() -> Value {
        json!({
            "category": "misc",
            "action": "approve",
            "fields": {
                "token": asset_ref("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
                "spender": "0x2222222222222222222222222222222222222222",
                "amount": amount_constraint("exact", "1000"),
                "approvalKind": "erc20"
            }
        })
    }

    fn swap_envelope_json() -> Value {
        json!({
            "category": "dex",
            "action": "swap",
            "fields": {
                "swapMode": "exact_in",
                "tokenIn": asset_ref("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18),
                "tokenOut": asset_ref("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
                "amountIn": amount_constraint("exact", "1000000000000000000"),
                "amountOut": amount_constraint("min", "900000000"),
                "recipient": "0x1111111111111111111111111111111111111111"
            }
        })
    }

    fn evaluate_envelope_input_json(envelope: Value) -> Value {
        json!({
            "envelope": envelope,
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x2222222222222222222222222222222222222222",
            "value_wei": "0",
            "chain_id": 1,
            "block_timestamp": 1_700_000_000_u64,
            "host_snapshot": empty_snapshot_json()
        })
    }

    #[test]
    fn evaluate_envelope_non_swap_returns_pass() {
        let output = evaluate_envelope_json(
            evaluate_envelope_input_json(approve_envelope_json()).to_string(),
        );
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "pass", "{parsed}");
    }

    #[test]
    fn evaluate_envelope_swap_returns_pass_under_empty_policy_set() {
        install_empty_policies_with_swap_schema();

        let output =
            evaluate_envelope_json(evaluate_envelope_input_json(swap_envelope_json()).to_string());
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "pass", "{parsed}");
    }

    #[test]
    fn evaluate_envelope_swap_routes_to_swap_action() {
        let install_output = install_policies_json(
            json!({
                "schema_text": "",
                "policy_set": [{
                    "id": "bundle::block-swap",
                    "text": r#"
                        @severity("deny")
                        @reason("swap blocked")
                        forbid (principal, action == Action::"swap", resource);
                    "#
                }]
            })
            .to_string(),
        );
        let install: Value = serde_json::from_str(&install_output).unwrap();
        assert_eq!(install["ok"], true, "{install}");

        let output =
            evaluate_envelope_json(evaluate_envelope_input_json(swap_envelope_json()).to_string());
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["kind"], "fail", "{parsed}");
        assert_eq!(
            parsed["data"]["matched"][0]["policy_id"], "bundle::block-swap",
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
}

// ── Phase 7: route_request_json ───────────────────────────────────────────────
// New-pipeline entry point exposing `request_router::route_request` to JS.
// Returns the `Vec<ActionEnvelope>` JSON inside the standard `{ok, data}` envelope.

#[derive(serde::Deserialize)]
struct RouteRequestInput {
    method: String,
    params: serde_json::Value,
    chain_id: u64,
    #[serde(default)]
    block_timestamp: Option<u64>,
}

#[wasm_bindgen]
pub fn route_request_json(input_json: String) -> String {
    let parse_result: Result<RouteRequestInput, _> = serde_json::from_str(&input_json);
    let input = match parse_result {
        Ok(v) => v,
        Err(e) => {
            return Envelope::<()>::err("invalid_input_json", format!("invalid input json: {e}"))
                .to_json();
        }
    };

    let registries = request_router::DefaultRegistries::standard();
    let token_registry = mappers::EmptyTokenRegistry;
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

#[wasm_bindgen]
pub fn evaluate_envelope_json(input_json: String) -> String {
    let verdict = (|| -> Result<Verdict, EngineErrorDto> {
        let input: EvaluateEnvelopeInputDto =
            serde_json::from_str(&input_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;
        let EvaluateEnvelopeInputDto {
            envelope,
            from,
            to,
            value_wei,
            chain_id,
            block_timestamp,
            host_snapshot,
        } = input;

        let envelope: ActionEnvelope = serde_json::from_value(envelope)
            .map_err(|error| EngineErrorDto::new("invalid_envelope_json", error.to_string()))?;
        let from: ActionAddress = from
            .parse()
            .map_err(|error| EngineErrorDto::new("invalid_from", error))?;
        let to: ActionAddress = to
            .parse()
            .map_err(|error| EngineErrorDto::new("invalid_to", error))?;
        let value_wei: DecimalString = value_wei
            .parse()
            .map_err(|error| EngineErrorDto::new("invalid_value_wei", error))?;

        // Build host capabilities from the snapshot DTO, then enrich the
        // envelope's swap action (if any) before lowering. Non-swap envelopes
        // pass through unchanged. Missing host data simply leaves enrichment
        // fields as `None`.
        let host_parts = host_capabilities_parts_from_dto(&host_snapshot);
        let host = host_parts.as_capabilities();
        let envelope = enrich_envelope(envelope, &from, &to, &host);

        let Some(request) = policy_request_from_envelope(
            &envelope,
            &from,
            &to,
            &value_wei,
            chain_id,
            block_timestamp,
        ) else {
            return Ok(Verdict::Pass);
        };

        STATE.with(|state| {
            let state = state.borrow();
            let state = state.as_ref().ok_or_else(not_installed_error)?;
            state
                .policies
                .evaluate_requests(std::iter::once((&request, PolicyRequestOrigin::Action)))
                .map_err(|error| EngineErrorDto::new("policy", error.to_string()))
        })
    })();

    let dto = match verdict {
        Ok(verdict) => verdict_to_dto(verdict),
        Err(error) => engine_error_verdict(error),
    };
    Envelope::ok(dto).to_json()
}

/// Owned storage for the host capability traits constructed from a snapshot
/// DTO. Held in this struct so the borrowed [`HostCapabilities`] can reference
/// the trait objects for the duration of a single `evaluate_envelope_json`
/// call.
struct HostCapabilityParts {
    oracle: SnapshotOracle,
    portfolio: Option<MockPortfolio>,
    approvals: Option<MockApprovals>,
}

impl HostCapabilityParts {
    fn as_capabilities(&self) -> HostCapabilities<'_> {
        let mut host = HostCapabilities::new(&self.oracle);
        if let Some(portfolio) = &self.portfolio {
            host = host.with_portfolio(portfolio);
        }
        if let Some(approvals) = &self.approvals {
            host = host.with_approvals(approvals);
        }
        host
    }
}

fn host_capabilities_parts_from_dto(snapshot: &HostSnapshotDto) -> HostCapabilityParts {
    let oracle = snapshot_oracle_from_entries(&snapshot.oracle);
    let portfolio = snapshot_portfolio_from_entries(&snapshot.balances);
    let approvals = snapshot_approvals_from_entries(&snapshot.allowances);
    HostCapabilityParts {
        oracle,
        portfolio,
        approvals,
    }
}

fn snapshot_oracle_from_entries(entries: &[OracleEntryDto]) -> SnapshotOracle {
    let mut oracle = SnapshotOracle::new();
    for entry in entries {
        let Some(token) = token_from_token_key(&entry.token_key) else {
            continue;
        };
        oracle.insert(
            &token,
            CoreUsdValuation {
                value: entry.usd_per_unit.clone(),
                as_of_ts: entry.as_of_ts,
                sources: entry.sources.clone(),
                stale_sec: entry.stale_sec,
            },
        );
    }
    oracle
}

fn snapshot_portfolio_from_entries(entries: &[BalanceEntryDto]) -> Option<MockPortfolio> {
    if entries.is_empty() {
        return None;
    }
    let mut portfolio = MockPortfolio::new();
    for entry in entries {
        let Some(owner) = CoreAddress::new(&entry.owner).ok() else {
            continue;
        };
        let Some(token) = token_from_token_key(&entry.token_key) else {
            continue;
        };
        let Some(balance) = U256::from_str_radix(&entry.balance, 10).ok() else {
            continue;
        };
        portfolio = portfolio.with_balance(&owner, &token, balance);
    }
    Some(portfolio)
}

fn snapshot_approvals_from_entries(entries: &[AllowanceEntryDto]) -> Option<MockApprovals> {
    if entries.is_empty() {
        return None;
    }
    let mut approvals = MockApprovals::new();
    for entry in entries {
        let Some(owner) = CoreAddress::new(&entry.owner).ok() else {
            continue;
        };
        let Some(spender) = CoreAddress::new(&entry.spender).ok() else {
            continue;
        };
        let Some(token) = token_from_token_key(&entry.token_key) else {
            continue;
        };
        let Some(allowance) = U256::from_str_radix(&entry.allowance, 10).ok() else {
            continue;
        };
        approvals = approvals.with_allowance(&owner, &token, &spender, allowance);
    }
    Some(approvals)
}

/// Parse a `Token::key()` of the form `"chain_id:address"` back into a
/// minimal `Token`. The `symbol` and `decimals` fields are not encoded in the
/// key — host trait lookups only consult `Token::key()` (chain_id + address),
/// so we stub the remaining fields. Enrichment paths that need real decimals
/// (e.g. USD scaling) read them from the envelope's `AssetRef.decimals`
/// instead, not from this reconstructed `Token`.
fn token_from_token_key(key: &str) -> Option<Token> {
    let (chain, address) = key.split_once(':')?;
    let chain_id = chain.parse::<u64>().ok()?;
    let address = CoreAddress::new(address).ok()?;
    Some(Token {
        chain_id,
        address,
        symbol: String::new(),
        decimals: 0,
        is_native: false,
    })
}
