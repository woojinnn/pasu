use alloy_primitives::U256;
use mappers::EmptyTokenRegistry;
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::{
    policy_request_from_envelope, Action, ActionAddress, ActionEnvelope, AmountConstraint,
    AmountKind, AssetKind, AssetRef, Category, DecimalString, HostCapabilities, MockOracle,
    PolicyEngineBuilder, PolicyRequest, Verdict,
};
use request_router::{route_request, DefaultRegistries, RouterContext};
use serde_json::Value;
use std::path::Path;
use std::str::FromStr as _;

const BLOCK_TIMESTAMP: u64 = 1_700_000_000;
const CORE_SCHEMA: &str = include_str!("../../../policy-schema/core.cedarschema");
const SWAP_SCHEMA: &str = include_str!("../../../policy-schema/actions/swap.cedarschema");

type HostSnapshot<'a> = HostCapabilities<'a>;

fn install_policies_and_evaluate(
    policies: &[(&str, &str)],
    schema_text: Option<&str>,
    request: &PolicyRequest,
    host_snapshot: &HostSnapshot<'_>,
) -> Verdict {
    let _ = host_snapshot;

    let mut builder = PolicyEngineBuilder::new();
    if let Some(schema_text) = schema_text.and_then(additive_schema_text) {
        builder = builder.add_schema_text(schema_text);
    }

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

fn additive_schema_text(schema_text: &str) -> Option<&str> {
    let schema_text = schema_text.trim();
    if schema_text.is_empty() {
        return None;
    }

    let extension = schema_text.strip_prefix(CORE_SCHEMA).unwrap_or(schema_text);
    let extension = extension.trim();
    if extension.is_empty() {
        None
    } else {
        Some(extension)
    }
}

fn deny_policy(policy_id: &str, policy_text: &str) -> String {
    format!("@id(\"{policy_id}\")\n@severity(\"deny\")\n{policy_text}\n")
}

fn schema_text() -> String {
    format!("{CORE_SCHEMA}\n{SWAP_SCHEMA}")
}

fn empty_host_snapshot<'a>(oracle: &'a MockOracle) -> HostSnapshot<'a> {
    HostCapabilities::new(oracle)
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
    let oracle = MockOracle::default();
    let host_snapshot = empty_host_snapshot(&oracle);

    let verdict = install_policies_and_evaluate(&[], None, &request, &host_snapshot);

    assert_eq!(verdict, Verdict::Pass);
}

#[test]
fn e2e_swap_v2_fails_under_blanket_forbid() {
    let request = swap_request_from_fixture("swap_uniswap_v2_exact_in.json");
    let oracle = MockOracle::default();
    let host_snapshot = empty_host_snapshot(&oracle);

    let verdict = install_policies_and_evaluate(
        &[(
            "test/forbid-swap",
            r#"forbid (principal, action == Action::"swap", resource);"#,
        )],
        None,
        &request,
        &host_snapshot,
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
    let oracle = MockOracle::default();
    let host_snapshot = empty_host_snapshot(&oracle);

    let verdict = install_policies_and_evaluate(
        &[(
            "test/forbid-exact-in-swap",
            r#"forbid (principal, action == Action::"swap", resource)
               when { context.swapMode == "exact_in" };"#,
        )],
        None,
        &request,
        &host_snapshot,
    );

    assert!(matches!(verdict, Verdict::Fail(_)));
}

#[test]
fn e2e_v2_exact_out_does_not_match_exact_in_only_policy() {
    let request = swap_request_from_fixture("swap_uniswap_v2_exact_out.json");
    let oracle = MockOracle::default();
    let host_snapshot = empty_host_snapshot(&oracle);

    let verdict = install_policies_and_evaluate(
        &[(
            "test/forbid-exact-in-swap",
            r#"forbid (principal, action == Action::"swap", resource)
               when { context.swapMode == "exact_in" };"#,
        )],
        None,
        &request,
        &host_snapshot,
    );

    assert_eq!(verdict, Verdict::Pass);
}

const MAX_FEE_POLICY: &str = include_str!("../../../policies/swap/max-fee-bps-100.cedar");
const NO_ZERO_MIN_OUTPUT_POLICY: &str =
    include_str!("../../../policies/swap/no-zero-min-output.cedar");

fn evaluate_with_policies(policies: &[&str], request: &PolicyRequest) -> Verdict {
    let mut builder = PolicyEngineBuilder::new();
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

fn synthetic_swap_request(fee_bps: u32) -> PolicyRequest {
    let from = ActionAddress::from_str("0x1111111111111111111111111111111111111111")
        .expect("valid synthetic from address");
    let to = ActionAddress::from_str("0x2222222222222222222222222222222222222222")
        .expect("valid synthetic to address");
    let token_in = AssetRef {
        kind: AssetKind::Erc20,
        chain_id: 1,
        address: Some(
            ActionAddress::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7")
                .expect("valid USDT address"),
        ),
        symbol: Some("USDT".to_owned()),
        decimals: Some(6),
    };
    let token_out = AssetRef {
        kind: AssetKind::Erc20,
        chain_id: 1,
        address: Some(
            ActionAddress::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")
                .expect("valid WETH address"),
        ),
        symbol: Some("WETH".to_owned()),
        decimals: Some(18),
    };
    let amount_in = AmountConstraint {
        kind: AmountKind::Exact,
        value: Some(DecimalString::from_str("10000000000").expect("valid amount-in")),
    };
    let amount_out = AmountConstraint {
        kind: AmountKind::Min,
        value: Some(DecimalString::from_str("200000000").expect("valid amount-out")),
    };
    let envelope = ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(SwapAction {
            mode: SwapMode::ExactIn,
            token_in,
            token_out,
            amount_in,
            amount_out,
            recipient: from.clone(),
            validity: None,
            fee_bps: Some(fee_bps),
            enrichment: SwapEnrichment::default(),
        }),
    };

    policy_request_from_envelope(
        &envelope,
        &from,
        &to,
        &DecimalString::from_str("0").expect("zero decimal"),
        1,
        BLOCK_TIMESTAMP,
    )
    .expect("synthetic swap envelope should lower to a policy request")
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

    let verdict =
        evaluate_with_policies(&[MAX_FEE_POLICY, NO_ZERO_MIN_OUTPUT_POLICY], &request);

    assert_eq!(verdict, Verdict::Pass);
}
