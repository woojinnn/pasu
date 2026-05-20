use alloy_primitives::U256;
use mappers::EmptyTokenRegistry;
use policy_engine::action::common::AssetRefWithAmountConstraint;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::policy_rpc::{
    apply_rpc_results, plan_calls, PolicyManifest, PolicyRpcResponse, PolicyRpcResult, RootInput,
};
use policy_engine::{
    policy_request_from_envelope, Action, ActionAddress, ActionEnvelope, AmountConstraint,
    AmountKind, AssetKind, AssetRef, Category, DecimalString, PolicyEngineBuilder, PolicyRequest,
    PolicySchemaComposer, Severity, Validity, ValiditySource, Verdict,
};
use request_router::{route_request, DefaultRegistries, RouterContext};
use serde_json::{json, Value};
use std::path::Path;
use std::str::FromStr as _;

const BLOCK_TIMESTAMP: u64 = 1_700_000_000;

fn install_policies_and_evaluate(policies: &[(&str, &str)], request: &PolicyRequest) -> Verdict {
    let mut builder = PolicyEngineBuilder::new();

    for (policy_id, policy_text) in policies {
        builder = builder.add_text(deny_policy(policy_id, policy_text));
    }

    let engine = builder
        .build()
        .unwrap_or_else(|error| panic!("policy engine should build: {error}"));

    engine
        .evaluate(
            &request.principal,
            &request.action,
            &request.resource,
            &request.entities,
            &request.context,
        )
        .unwrap_or_else(|error| panic!("policy request should evaluate: {error}"))
}

fn deny_policy(policy_id: &str, policy_text: &str) -> String {
    format!("@id(\"{policy_id}\")\n@severity(\"deny\")\n{policy_text}\n")
}

fn load_fixture(filename: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("golden")
        .join("inputs")
        .join(filename);
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read fixture {}: {error}", path.display()));
    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("failed to parse fixture {}: {error}", path.display()))
}

fn policy_request_from_fixture(filename: &str) -> Option<PolicyRequest> {
    let fixture = load_fixture(filename);
    let rpc = fixture
        .get("rpc")
        .unwrap_or_else(|| panic!("{filename} missing rpc object"));
    let method = rpc
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{filename} missing rpc.method"));
    let params = rpc
        .get("params")
        .unwrap_or_else(|| panic!("{filename} missing rpc.params"));
    let chain_id = fixture
        .get("chain_id")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("{filename} missing chain_id"));

    let registries = DefaultRegistries::standard();
    let token_registry = EmptyTokenRegistry;
    let context = RouterContext {
        registries: &registries,
        token_registry: &token_registry,
        block_timestamp: Some(BLOCK_TIMESTAMP),
    };
    let envelopes = route_request(&context, method, params, chain_id)
        .unwrap_or_else(|error| panic!("{filename} should route through request_router: {error}"));
    assert_eq!(envelopes.len(), 1, "{filename} should emit one envelope");

    let tx = params
        .as_array()
        .and_then(|params| params.first())
        .unwrap_or_else(|| panic!("{filename} missing params[0] transaction"));
    let from = address_field(tx, "from", filename);
    let to = address_field(tx, "to", filename);
    let value_wei = tx_value_wei(tx, filename);

    policy_request_from_envelope(
        &envelopes[0],
        &from,
        &to,
        &value_wei,
        chain_id,
        BLOCK_TIMESTAMP,
    )
}

fn address_field(tx: &Value, field: &str, filename: &str) -> ActionAddress {
    let value = tx
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{filename} missing tx.{field}"));
    ActionAddress::from_str(value)
        .unwrap_or_else(|error| panic!("{filename} has invalid tx.{field}: {error}"))
}

fn tx_value_wei(tx: &Value, filename: &str) -> DecimalString {
    let raw = tx.get("value").and_then(Value::as_str).unwrap_or("0");
    let value = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        if hex.is_empty() {
            "0".to_owned()
        } else {
            U256::from_str_radix(hex, 16)
                .unwrap_or_else(|error| panic!("{filename} has invalid tx.value: {error}"))
                .to_string()
        }
    } else {
        raw.to_owned()
    };

    DecimalString::from_str(&value)
        .unwrap_or_else(|error| panic!("{filename} has invalid tx.value: {error}"))
}

fn swap_request_from_fixture(filename: &str) -> PolicyRequest {
    policy_request_from_fixture(filename)
        .unwrap_or_else(|| panic!("{filename} should lower to a swap policy request"))
}

#[test]
fn e2e_swap_v2_passes_under_empty_policies() {
    let request = swap_request_from_fixture("swap_uniswap_v2_exact_in.json");

    let verdict = install_policies_and_evaluate(&[], &request);

    assert_eq!(verdict, Verdict::Pass);
}

#[test]
fn e2e_swap_v2_deadline_carries_validity_through_lowering() {
    // Post-Phase-2 `validityDeltaSec` is manifest-driven, not host-derived.
    // The lowering passes the raw validity block through; the matching
    // manifest produces the delta via a `clock.validity_delta_sec` call.
    let request = swap_request_from_fixture("swap_uniswap_v2_exact_in.json");

    let validity = request
        .context
        .get("validity")
        .and_then(Value::as_object)
        .expect("lowering should carry the validity block through to context");
    assert_eq!(
        validity
            .get("expiresAt")
            .and_then(Value::as_str)
            .expect("validity.expiresAt is a decimal string"),
        "9999999999"
    );
    assert!(!request
        .context
        .as_object()
        .expect("context is an object")
        .contains_key("validityDeltaSec"));
}

#[test]
fn e2e_swap_v2_fails_under_blanket_forbid() {
    let request = swap_request_from_fixture("swap_uniswap_v2_exact_in.json");

    let verdict = install_policies_and_evaluate(
        &[(
            "test/forbid-swap",
            r#"forbid (principal, action == Action::"swap", resource);"#,
        )],
        &request,
    );

    match verdict {
        Verdict::Fail(matched) => {
            assert!(matched
                .iter()
                .any(|policy| policy.policy_id.contains("forbid")));
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn e2e_approve_action_is_unsupported_for_now() {
    let request = policy_request_from_fixture("erc20_approve.json");

    assert!(request.is_none());
}

#[test]
fn e2e_swap_v3_evaluates_through_new_pipeline() {
    let request = swap_request_from_fixture("swap_uniswap_v3_exact_input_single.json");

    let verdict = install_policies_and_evaluate(
        &[(
            "test/forbid-exact-in-swap",
            r#"forbid (principal, action == Action::"swap", resource)
               when { context.swapMode == "exact_in" };"#,
        )],
        &request,
    );

    assert!(matches!(verdict, Verdict::Fail(_)));
}

#[test]
fn e2e_v2_exact_out_does_not_match_exact_in_only_policy() {
    let request = swap_request_from_fixture("swap_uniswap_v2_exact_out.json");

    let verdict = install_policies_and_evaluate(
        &[(
            "test/forbid-exact-in-swap",
            r#"forbid (principal, action == Action::"swap", resource)
               when { context.swapMode == "exact_in" };"#,
        )],
        &request,
    );

    assert_eq!(verdict, Verdict::Pass);
}

const MAX_FEE_POLICY: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/max-fee-bps-100.cedar");
const NO_ZERO_MIN_OUTPUT_POLICY: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/no-zero-min-output.cedar");
const MAX_INPUT_USD_100_POLICY: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/max-input-usd-100.cedar");
const MAX_INPUT_USD_100_MANIFEST: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/max-input-usd-100.policy-rpc.json");
const MIN_OUTPUT_USD_FLOOR_POLICY: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/min-output-usd-floor.cedar");
const MIN_OUTPUT_USD_FLOOR_MANIFEST: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/min-output-usd-floor.policy-rpc.json");
const KNOWN_TOKEN_ONLY_POLICY: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/known-token-only.cedar");
const MAX_FEE_BPS_30_POLICY: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/max-fee-bps-30.cedar");
const EXPIRED_DEADLINE_POLICY: &str =
    include_str!("../../../policy-rpc/examples/policies/swap/expired-deadline.cedar");

fn evaluate_with_policies(policies: &[&str], request: &PolicyRequest) -> Verdict {
    evaluate_with_policies_and_manifests(policies, &[], request)
}

fn evaluate_with_policies_and_manifests(
    policies: &[&str],
    manifests: &[PolicyManifest],
    request: &PolicyRequest,
) -> Verdict {
    // D3: when one or more manifests are supplied, build the schema via the
    // enriched composer so that manifest outputs land inside the per-action
    // `custom?: <Action>CustomContext` namespace (matching where the
    // materializer writes them at runtime). For the no-manifest case fall
    // back to the legacy `PolicySchemaComposer` (used by tests that don't
    // declare any RPC-driven extension).
    let schema = if manifests.is_empty() {
        PolicySchemaComposer::new()
            .with_manifests(manifests)
            .expect("base schema composes")
            .compose()
    } else {
        let mut grouped: std::collections::BTreeMap<String, PolicyManifest> =
            std::collections::BTreeMap::new();
        for manifest in manifests {
            for requirement in &manifest.requires {
                let action = requirement.when.action.clone();
                grouped.entry(action).or_insert_with(|| manifest.clone());
            }
        }
        policy_engine::schema::compose_enriched(&grouped)
            .expect("manifests compose into enriched schema")
            .schema_text
    };
    let mut builder = PolicyEngineBuilder::with_schema_text(schema);
    for policy_text in policies {
        builder = builder.add_text(*policy_text);
    }
    let engine = builder
        .build()
        .unwrap_or_else(|error| panic!("policy engine should build: {error}"));
    engine
        .evaluate(
            &request.principal,
            &request.action,
            &request.resource,
            &request.entities,
            &request.context,
        )
        .unwrap_or_else(|error| panic!("policy request should evaluate: {error}"))
}

struct SyntheticSwapInput<'a> {
    token_in_kind: AssetKind,
    token_in_symbol: &'a str,
    token_out_symbol: &'a str,
    amount_out_kind: AmountKind,
    fee_bps: Option<u32>,
    total_input_usd: Option<&'a str>,
    total_min_output_usd: Option<&'a str>,
    validity_delta_sec: Option<i64>,
}

impl<'a> Default for SyntheticSwapInput<'a> {
    fn default() -> Self {
        Self {
            token_in_kind: AssetKind::Erc20,
            token_in_symbol: "USDC",
            token_out_symbol: "ETH",
            amount_out_kind: AmountKind::Min,
            fee_bps: None,
            total_input_usd: None,
            total_min_output_usd: None,
            validity_delta_sec: None,
        }
    }
}

fn synthetic_swap_request(fee_bps: u32) -> PolicyRequest {
    synthetic_swap_request_with(SyntheticSwapInput {
        fee_bps: Some(fee_bps),
        ..SyntheticSwapInput::default()
    })
}

fn synthetic_swap_request_with(input: SyntheticSwapInput<'_>) -> PolicyRequest {
    let from = ActionAddress::from_str("0x1111111111111111111111111111111111111111")
        .expect("valid synthetic from address");
    let to = ActionAddress::from_str("0x2222222222222222222222222222222222222222")
        .expect("valid synthetic to address");
    let token_in = AssetRef {
        kind: input.token_in_kind,
        address: Some(
            ActionAddress::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
                .expect("valid USDC address"),
        ),
        token_id: None,
        symbol: Some(input.token_in_symbol.to_owned()),
        decimals: Some(6),
    };
    let token_out = AssetRef {
        kind: AssetKind::Erc20,
        address: Some(
            ActionAddress::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")
                .expect("valid WETH address"),
        ),
        token_id: None,
        symbol: Some(input.token_out_symbol.to_owned()),
        decimals: Some(18),
    };
    let amount_in = AmountConstraint {
        kind: AmountKind::Exact,
        value: Some(DecimalString::from_str("10000000000").expect("valid amount-in")),
    };
    let amount_out = AmountConstraint {
        kind: input.amount_out_kind,
        value: Some(DecimalString::from_str("200000000").expect("valid amount-out")),
    };
    let validity = input.validity_delta_sec.map(|delta_sec| Validity {
        expires_at: DecimalString::from_str(&(BLOCK_TIMESTAMP as i64 + delta_sec).to_string())
            .expect("valid synthetic expiry"),
        source: ValiditySource::TxDeadline,
    });
    let envelope = ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(SwapAction {
            swap_mode: SwapMode::ExactIn,
            input_token: AssetRefWithAmountConstraint {
                asset: token_in,
                amount: amount_in,
            },
            output_token: AssetRefWithAmountConstraint {
                asset: token_out,
                amount: amount_out,
            },
            recipient: from.clone(),
            validity,
            fee_bps: input.fee_bps,
        }),
    };

    let mut requests = vec![policy_request_from_envelope(
        &envelope,
        &from,
        &to,
        &DecimalString::from_str("0").expect("zero decimal"),
        1,
        BLOCK_TIMESTAMP,
    )
    .expect("synthetic swap envelope should lower to a policy request")];

    materialize_synthetic_rpc_context(
        &mut requests,
        &envelope,
        &from,
        &to,
        input.total_input_usd,
        input.total_min_output_usd,
    );

    requests.remove(0)
}

fn materialize_synthetic_rpc_context(
    requests: &mut [PolicyRequest],
    envelope: &ActionEnvelope,
    from: &ActionAddress,
    to: &ActionAddress,
    total_input_usd: Option<&str>,
    total_min_output_usd: Option<&str>,
) {
    let mut manifests = Vec::new();
    if total_input_usd.is_some() {
        manifests.push(policy_manifest(MAX_INPUT_USD_100_MANIFEST));
    }
    if total_min_output_usd.is_some() {
        manifests.push(policy_manifest(MIN_OUTPUT_USD_FLOOR_MANIFEST));
    }
    if manifests.is_empty() {
        return;
    }

    let root = RootInput {
        chain_id: 1,
        from: from.to_string(),
        to: to.to_string(),
        value_wei: "0".to_owned(),
        block_timestamp: Some(BLOCK_TIMESTAMP),
    };
    let calls = plan_calls(
        &root,
        std::slice::from_ref(envelope),
        &manifests,
        &json!({}),
    )
    .expect("synthetic manifests should plan");
    let results = calls
        .into_iter()
        .map(|call| {
            let value = if call.id.contains("swap-min-output-usd") {
                total_min_output_usd.expect("min output result value")
            } else {
                total_input_usd.expect("input result value")
            };
            PolicyRpcResult {
                id: call.id,
                ok: true,
                result: Some(policy_rpc_usd_result(value)),
                error: None,
            }
        })
        .collect();

    apply_rpc_results(
        requests,
        std::slice::from_ref(envelope),
        &manifests,
        &PolicyRpcResponse {
            request_id: "synthetic-eval".to_owned(),
            results,
        },
    )
    .expect("synthetic RPC response should materialize");
}

fn policy_manifest(text: &str) -> PolicyManifest {
    serde_json::from_str(text).expect("default policy RPC manifest parses")
}

fn policy_rpc_usd_result(value: &str) -> Value {
    json!({
        "value": value,
        "asOfTs": BLOCK_TIMESTAMP,
        "sources": ["synthetic"],
        "staleSec": 0
    })
}

fn assert_policy_passes(policy_text: &str, request: &PolicyRequest) {
    let verdict = evaluate_with_policies(&[policy_text], request);

    assert_eq!(verdict, Verdict::Pass);
}

fn assert_policy_passes_with_manifest(
    policy_text: &str,
    manifest_text: &str,
    request: &PolicyRequest,
) {
    let manifest = policy_manifest(manifest_text);
    let verdict = evaluate_with_policies_and_manifests(
        &[policy_text],
        std::slice::from_ref(&manifest),
        request,
    );

    assert_eq!(verdict, Verdict::Pass);
}

fn assert_policy_denies(policy_text: &str, request: &PolicyRequest, policy_id: &str) {
    let verdict = evaluate_with_policies(&[policy_text], request);

    match verdict {
        Verdict::Fail(matched) => {
            assert!(
                matched.iter().any(
                    |policy| policy.policy_id == policy_id && policy.severity == Severity::Deny
                ),
                "expected deny policy {policy_id} to fire, got {matched:?}"
            );
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

fn assert_policy_denies_with_manifest(
    policy_text: &str,
    manifest_text: &str,
    request: &PolicyRequest,
    policy_id: &str,
) {
    let manifest = policy_manifest(manifest_text);
    let verdict = evaluate_with_policies_and_manifests(
        &[policy_text],
        std::slice::from_ref(&manifest),
        request,
    );

    match verdict {
        Verdict::Fail(matched) => {
            assert!(
                matched.iter().any(
                    |policy| policy.policy_id == policy_id && policy.severity == Severity::Deny
                ),
                "expected deny policy {policy_id} to fire, got {matched:?}"
            );
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn test_max_input_usd_100_pass() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        total_input_usd: Some("50.0000"),
        ..SyntheticSwapInput::default()
    });

    assert_policy_passes_with_manifest(
        MAX_INPUT_USD_100_POLICY,
        MAX_INPUT_USD_100_MANIFEST,
        &request,
    );
}

#[test]
fn test_max_input_usd_100_fail() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        total_input_usd: Some("200.0000"),
        ..SyntheticSwapInput::default()
    });

    assert_policy_denies_with_manifest(
        MAX_INPUT_USD_100_POLICY,
        MAX_INPUT_USD_100_MANIFEST,
        &request,
        "user/max-input-usd-100",
    );
}

#[test]
fn test_min_output_usd_floor_pass() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        total_min_output_usd: Some("75.0000"),
        ..SyntheticSwapInput::default()
    });

    assert_policy_passes_with_manifest(
        MIN_OUTPUT_USD_FLOOR_POLICY,
        MIN_OUTPUT_USD_FLOOR_MANIFEST,
        &request,
    );
}

#[test]
fn test_min_output_usd_floor_fail() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        total_min_output_usd: Some("25.0000"),
        ..SyntheticSwapInput::default()
    });

    assert_policy_denies_with_manifest(
        MIN_OUTPUT_USD_FLOOR_POLICY,
        MIN_OUTPUT_USD_FLOOR_MANIFEST,
        &request,
        "user/min-output-usd-floor",
    );
}

#[test]
fn test_known_token_only_pass() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        token_in_symbol: "USDC",
        token_out_symbol: "ETH",
        ..SyntheticSwapInput::default()
    });

    assert_policy_passes(KNOWN_TOKEN_ONLY_POLICY, &request);
}

#[test]
fn test_known_token_only_fail() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        token_in_symbol: "",
        token_out_symbol: "ETH",
        ..SyntheticSwapInput::default()
    });

    assert_policy_denies(KNOWN_TOKEN_ONLY_POLICY, &request, "user/known-token-only");
}

#[test]
fn test_max_fee_bps_30_pass() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        fee_bps: Some(10),
        ..SyntheticSwapInput::default()
    });

    assert_policy_passes(MAX_FEE_BPS_30_POLICY, &request);
}

#[test]
fn test_max_fee_bps_30_fail() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        fee_bps: Some(50),
        ..SyntheticSwapInput::default()
    });

    assert_policy_denies(MAX_FEE_BPS_30_POLICY, &request, "user/max-fee-bps-30");
}

#[test]
fn test_expired_deadline_pass() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        validity_delta_sec: Some(300),
        ..SyntheticSwapInput::default()
    });

    assert_policy_passes(EXPIRED_DEADLINE_POLICY, &request);
}

#[test]
#[ignore = "TODO(phase-5/D11): EXPIRED_DEADLINE_POLICY now reads context.custom.validityDeltaSec but the lowering no longer derives validityDeltaSec host-side and no manifest is wired in for this test. Re-enable after the materializer writes manifest outputs into context.custom and a synthetic manifest plumbs validityDeltaSec there."]
fn test_expired_deadline_fail() {
    let request = synthetic_swap_request_with(SyntheticSwapInput {
        validity_delta_sec: Some(0),
        ..SyntheticSwapInput::default()
    });

    assert_policy_denies(EXPIRED_DEADLINE_POLICY, &request, "user/expired-deadline");
}

#[test]
fn swap_fails_when_fee_bps_exceeds_cap() {
    // No fixture has fee_bps > 100 (V3 tiers are 1/5/30/100); construct a
    // synthetic envelope with fee_bps = 500 to exercise the max-fee-bps-100
    // policy end-to-end through policy_request_from_envelope.
    let request = synthetic_swap_request(500);

    let verdict = evaluate_with_policies(&[MAX_FEE_POLICY], &request);

    match verdict {
        Verdict::Fail(matched) => {
            assert!(
                matched
                    .iter()
                    .any(|policy| policy.policy_id.contains("max-fee-bps-100")),
                "expected max-fee-bps-100 policy to fire, got {matched:?}"
            );
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn swap_passes_under_max_fee_policy() {
    // V2 swapExactETHForTokens fixture has fee 30 bps (well under the 100
    // bps cap) and a non-zero amountOutMin (0x0bebc200 = 200_000_000), so
    // both swap-only policies should leave the verdict at Pass.
    let request = swap_request_from_fixture("swap_uniswap_v2_exact_eth_for_tokens.json");

    let verdict = evaluate_with_policies(&[MAX_FEE_POLICY, NO_ZERO_MIN_OUTPUT_POLICY], &request);

    assert_eq!(verdict, Verdict::Pass);
}

/// End-to-end: route a v2 swap fixture, plan the default Policy RPC manifest,
/// materialize its JSON result, and assert that the resulting Cedar context
/// carries `totalInputUsd`.
#[test]
fn policy_rpc_materializes_usd_valuation_on_v2_swap() {
    let fixture = load_fixture("swap_uniswap_v2_exact_in.json");
    let rpc = fixture.get("rpc").unwrap();
    let method = rpc.get("method").and_then(Value::as_str).unwrap();
    let params = rpc.get("params").unwrap();
    let chain_id = fixture.get("chain_id").and_then(Value::as_u64).unwrap();

    let registries = DefaultRegistries::standard();
    let token_registry = EmptyTokenRegistry;
    let ctx = RouterContext {
        registries: &registries,
        token_registry: &token_registry,
        block_timestamp: Some(BLOCK_TIMESTAMP),
    };
    let envelopes = route_request(&ctx, method, params, chain_id).expect("route");
    assert_eq!(envelopes.len(), 1);
    let envelope = envelopes[0].clone();

    let tx = params.as_array().unwrap().first().unwrap();
    let from = address_field(tx, "from", "swap_uniswap_v2_exact_in.json");
    let to = address_field(tx, "to", "swap_uniswap_v2_exact_in.json");
    let mut requests = vec![policy_request_from_envelope(
        &envelope,
        &from,
        &to,
        &DecimalString::from_str("0").unwrap(),
        chain_id,
        BLOCK_TIMESTAMP,
    )
    .expect("envelope should lower to a swap policy request")];
    let manifest = policy_manifest(MAX_INPUT_USD_100_MANIFEST);
    let root = RootInput {
        chain_id,
        from: from.to_string(),
        to: to.to_string(),
        value_wei: "0".to_owned(),
        block_timestamp: Some(BLOCK_TIMESTAMP),
    };
    let call = plan_calls(
        &root,
        std::slice::from_ref(&envelope),
        std::slice::from_ref(&manifest),
        &json!({}),
    )
    .expect("default manifest should plan")
    .pop()
    .expect("manifest should produce one call");

    apply_rpc_results(
        &mut requests,
        std::slice::from_ref(&envelope),
        std::slice::from_ref(&manifest),
        &PolicyRpcResponse {
            request_id: "fixture-eval".to_owned(),
            results: vec![PolicyRpcResult {
                id: call.id,
                ok: true,
                result: Some(policy_rpc_usd_result("125.0000")),
                error: None,
            }],
        },
    )
    .expect("default manifest response should materialize");

    // D3: manifest outputs land under `context.custom.<field>`, not at the
    // top level.
    assert!(
        requests[0]
            .context
            .get("custom")
            .and_then(|c| c.get("totalInputUsd"))
            .is_some(),
        "policy request context should include custom.totalInputUsd after Policy RPC, got: {}",
        requests[0].context
    );
}
