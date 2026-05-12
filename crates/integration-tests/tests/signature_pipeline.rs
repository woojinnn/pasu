use policy_engine::{
    Action, ActionAdapterError, ActionAdapterId, Address, Eip2612Action, HostCapabilities,
    MockClock, MockOracle, MockSignatureActionAdapterRegistry,
    MockTransactionActionAdapterRegistry, Permit2Action, Permit2Approval, Permit2PermitKind,
    Pipeline, PipelineError, PolicyEngine, Request, SignatureActionAdapter, SignatureMatchKey,
    SignatureRequest, Token, TransactionRequest, Verdict,
};
use policy_engine_adapters_bundle::default_signature_registry;
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc};

const PERMIT2_SPENDER_ALLOWLIST: &str =
    include_str!("../../../policies/signature/_shared/spender-allowlist.cedar");
const PERMIT2_NO_UNLIMITED: &str =
    include_str!("../../../policies/signature/_shared/no-unlimited-amount.cedar");
const PERMIT2_MAX_USD: &str = include_str!("../../../policies/signature/_shared/max-usd-100.cedar");
const PERMIT2_DEADLINE: &str =
    include_str!("../../../policies/signature/permit2/sig-deadline-le-1h.cedar");
const PERMIT2_CHAIN: &str =
    include_str!("../../../policies/signature/_shared/chain-must-match.cedar");
const PERMIT2_NONCE: &str = include_str!("../../../policies/signature/_shared/nonce-sanity.cedar");
const PERMIT2_HUMAN_MAX: &str =
    include_str!("../../../policies/signature/permit2/max-human-amount-50.cedar");
const PERMIT2_WITNESS_BLOCKLIST: &str =
    include_str!("../../../policies/signature/permit2/witness-blocklist.cedar");

const EIP2612_SPENDER_ALLOWLIST: &str =
    include_str!("../../../policies/signature/_shared/spender-allowlist.cedar");
const EIP2612_NO_UNLIMITED: &str =
    include_str!("../../../policies/signature/_shared/no-unlimited-amount.cedar");
const EIP2612_MAX_USD: &str = include_str!("../../../policies/signature/_shared/max-usd-100.cedar");
const EIP2612_DEADLINE: &str =
    include_str!("../../../policies/signature/eip2612/deadline-le-1h.cedar");
const EIP2612_CHAIN: &str =
    include_str!("../../../policies/signature/_shared/chain-must-match.cedar");
const EIP2612_NONCE: &str = include_str!("../../../policies/signature/_shared/nonce-sanity.cedar");
const EIP2612_HUMAN_MAX: &str =
    include_str!("../../../policies/signature/eip2612/max-human-value-50.cedar");

const OTHER_VERIFYING_ALLOWLIST: &str =
    include_str!("../../../policies/signature/eip712-other/verifying-contract-allowlist.cedar");
const OTHER_CHAIN: &str =
    include_str!("../../../policies/signature/_shared/chain-must-match.cedar");

const UINT160_MAX: &str = "1461501637330902918203684832716283019655932542975";
const UINT256_MAX: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";
const USDC_ADDRESS: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

fn load_signature(name: &str) -> SignatureRequest {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/signatures")
        .join(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse fixture {}: {err}", path.display()))
}

fn permit2_single() -> SignatureRequest {
    load_signature("permit2_permit_single.json")
}

fn permit2_batch() -> SignatureRequest {
    load_signature("permit2_permit_batch.json")
}

fn permit2_batch_transfer() -> SignatureRequest {
    load_signature("permit2_permit_batch_transfer_from.json")
}

fn permit2_witness_transfer() -> SignatureRequest {
    load_signature("permit2_permit_witness_transfer_from.json")
}

fn eip2612_permit() -> SignatureRequest {
    load_signature("eip2612_permit.json")
}

fn eip712_other() -> SignatureRequest {
    load_signature("eip712_other_mail.json")
}

fn weth() -> Token {
    Token {
        chain_id: 1,
        address: Address::new("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
        symbol: "WETH".into(),
        decimals: 18,
        is_native: false,
    }
}

fn usdc() -> Token {
    Token {
        chain_id: 1,
        address: Address::new("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        symbol: "USDC".into(),
        decimals: 6,
        is_native: false,
    }
}

fn oracle() -> MockOracle {
    MockOracle::new()
        .with_simple_price(&weth(), "3000.0000", 5)
        .with_simple_price(&usdc(), "1.0000", 5)
}

fn evaluate(sig: SignatureRequest, policy: &str) -> Verdict {
    let clock = MockClock::with_fixed(1000);
    evaluate_with_clock(sig, policy, &clock)
}

fn evaluate_result(sig: SignatureRequest, policy: &str) -> Result<Verdict, PipelineError> {
    let clock = MockClock::with_fixed(1000);
    evaluate_result_with_registry(sig, policy, &clock, &default_signature_registry())
}

fn evaluate_with_clock(sig: SignatureRequest, policy: &str, clock: &MockClock) -> Verdict {
    evaluate_result_with_registry(sig, policy, clock, &default_signature_registry())
        .expect("pipeline ok")
}

fn evaluate_result_with_registry(
    sig: SignatureRequest,
    policy: &str,
    clock: &MockClock,
    sig_registry: &dyn policy_engine::SignatureActionAdapterRegistry,
) -> Result<Verdict, PipelineError> {
    let tx_registry = MockTransactionActionAdapterRegistry::new();
    let oracle = oracle();
    let policies = PolicyEngine::from_sources([policy]).expect("policy source parses");
    let pipe = Pipeline::new(
        &tx_registry,
        HostCapabilities::new(&oracle).with_clock(clock),
        &policies,
    )
    .with_signature_registry(sig_registry);

    pipe.evaluate(&Request::Sig(sig))
}

fn assert_fail_id(verdict: Verdict, policy_id: &str) {
    match verdict {
        Verdict::Fail(matched) => assert!(
            matched.iter().any(|policy| policy.policy_id == policy_id),
            "expected {policy_id} in {matched:?}"
        ),
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

fn permit2_set_details(field: &str, value: Value) -> SignatureRequest {
    let mut sig = permit2_single();
    sig.typed_data.message["details"][field] = value;
    sig
}

fn permit2_set_message(field: &str, value: Value) -> SignatureRequest {
    let mut sig = permit2_single();
    sig.typed_data.message[field] = value;
    sig
}

fn permit2_single_usdc_amount(raw_amount: &str) -> SignatureRequest {
    let mut sig = permit2_single();
    sig.typed_data.message["details"]["token"] = json!(USDC_ADDRESS);
    sig.typed_data.message["details"]["amount"] = json!(raw_amount);
    sig
}

fn eip2612_set_message(field: &str, value: Value) -> SignatureRequest {
    let mut sig = eip2612_permit();
    sig.typed_data.message[field] = value;
    sig
}

fn assert_lowering_error(error: &PipelineError, expected: &str) {
    match error {
        PipelineError::Lowering(message) => assert!(
            message.contains(expected),
            "expected {expected:?} in lowering error {message:?}"
        ),
        other => panic!("expected PipelineError::Lowering, got {other:?}"),
    }
}

#[test]
fn permit2_spender_allowlist_passes_and_fails() {
    assert_eq!(
        evaluate(permit2_single(), PERMIT2_SPENDER_ALLOWLIST),
        Verdict::Pass
    );
    let denied = permit2_set_message(
        "spender",
        json!("0x2222222222222222222222222222222222222222"),
    );
    assert_fail_id(
        evaluate(denied, PERMIT2_SPENDER_ALLOWLIST),
        "user/signature/permit2/spender-allowlist",
    );
}

#[test]
fn permit2_batch_transfer_from_spender_allowlist_fails_for_unlisted_spender() {
    let mut denied = permit2_batch_transfer();
    denied.typed_data.message["spender"] = json!("0x2222222222222222222222222222222222222222");

    assert_fail_id(
        evaluate(denied, PERMIT2_SPENDER_ALLOWLIST),
        "user/signature/permit2/spender-allowlist",
    );
}

#[test]
fn permit2_witness_transfer_fails_witness_blocklist() {
    assert_fail_id(
        evaluate(permit2_witness_transfer(), PERMIT2_WITNESS_BLOCKLIST),
        "user/signature/permit2/witness-blocklist",
    );
}

#[test]
fn permit2_no_unlimited_amount_passes_and_fails() {
    assert_eq!(
        evaluate(permit2_single(), PERMIT2_NO_UNLIMITED),
        Verdict::Pass
    );
    let denied = permit2_set_details("amount", json!(UINT160_MAX));
    assert_fail_id(
        evaluate(denied, PERMIT2_NO_UNLIMITED),
        "user/signature/permit2/no-unlimited-amount",
    );
}

#[test]
fn permit2_usd_cap_passes_and_fails() {
    assert_eq!(evaluate(permit2_single(), PERMIT2_MAX_USD), Verdict::Pass);
    let denied = permit2_set_details("amount", json!("100000000000000000"));
    assert_fail_id(
        evaluate(denied, PERMIT2_MAX_USD),
        "user/signature/permit2/max-usd-100",
    );
}

#[test]
fn permit2_deadline_window_passes_and_fails() {
    assert_eq!(evaluate(permit2_single(), PERMIT2_DEADLINE), Verdict::Pass);
    let denied = permit2_set_message("sigDeadline", json!(5000));
    assert_fail_id(
        evaluate(denied, PERMIT2_DEADLINE),
        "user/signature/permit2/sig-deadline-le-1h",
    );
}

#[test]
fn permit2_chain_match_passes_and_fails() {
    assert_eq!(evaluate(permit2_single(), PERMIT2_CHAIN), Verdict::Pass);
    let mut denied = permit2_single();
    denied.chain_id = 137;
    assert_fail_id(
        evaluate(denied, PERMIT2_CHAIN),
        "user/signature/permit2/chain-must-match",
    );
}

#[test]
fn permit2_nonce_sanity_passes_and_fails() {
    assert_eq!(evaluate(permit2_single(), PERMIT2_NONCE), Verdict::Pass);
    let denied = permit2_set_details("nonce", json!(UINT256_MAX));
    assert_fail_id(
        evaluate(denied, PERMIT2_NONCE),
        "user/signature/permit2/nonce-sanity",
    );
}

#[test]
fn permit2_human_amount_cap_passes_and_fails() {
    assert_eq!(
        evaluate(permit2_single_usdc_amount("10000000"), PERMIT2_HUMAN_MAX),
        Verdict::Pass
    );
    let denied = permit2_single_usdc_amount("100000000");
    assert_fail_id(
        evaluate(denied, PERMIT2_HUMAN_MAX),
        "user/signature/permit2/max-human-amount-50",
    );
}

#[test]
fn typed_data_validation_rejects_missing_eip712_domain_type() {
    let mut sig = permit2_single();
    sig.typed_data
        .types
        .as_object_mut()
        .expect("fixture types is object")
        .remove("EIP712Domain");

    let error =
        evaluate_result(sig, "").expect_err("missing EIP712Domain definition must fail early");

    assert_lowering_error(&error, "MissingEip712Domain");
}

#[test]
fn typed_data_validation_rejects_missing_primary_type() {
    let mut sig = permit2_single();
    sig.typed_data.primary_type = "MissingPrimaryType".into();

    let error = evaluate_result(sig, "")
        .expect_err("missing primaryType definition must fail at pipeline boundary");

    assert_lowering_error(&error, "types missing primaryType MissingPrimaryType");
}

#[test]
fn typed_data_validation_rejects_missing_message_field() {
    let mut sig = permit2_single();
    sig.typed_data
        .message
        .as_object_mut()
        .expect("fixture message is object")
        .remove("spender");

    let error =
        evaluate_result(sig, "").expect_err("missing declared message field must fail early");

    assert_lowering_error(&error, "message missing primaryType field spender");
}

#[test]
fn typed_data_validation_rejects_invalid_solidity_type() {
    let mut sig = permit2_single();
    sig.typed_data.types["PermitSingle"][2]["type"] = json!("uint257");

    let error = evaluate_result(sig, "").expect_err("invalid Solidity type must fail early");

    assert_lowering_error(&error, "InvalidType");
    assert_lowering_error(&error, "uint257");
}

#[test]
fn typed_data_validation_rejects_missing_referenced_type() {
    let mut sig = permit2_single();
    sig.typed_data.types["PermitDetails"][0]["type"] = json!("blockchain");

    let error = evaluate_result(sig, "").expect_err("missing referenced type must fail early");

    assert_lowering_error(&error, "MissingReferencedType");
    assert_lowering_error(&error, "blockchain");
}

#[test]
fn typed_data_validation_rejects_type_cycle() {
    let mut sig = eip712_other();
    sig.typed_data.primary_type = "Node".into();
    sig.typed_data.types = json!({
        "EIP712Domain": [
            { "name": "name", "type": "string" },
            { "name": "version", "type": "string" },
            { "name": "chainId", "type": "uint256" },
            { "name": "verifyingContract", "type": "address" }
        ],
        "Node": [
            { "name": "children", "type": "Node[]" }
        ]
    });
    sig.typed_data.message = json!({
        "children": []
    });

    let error = evaluate_result(sig, "").expect_err("type cycle must fail early");

    assert_lowering_error(&error, "TypeCycle");
    assert_lowering_error(&error, "Node");
}

#[test]
fn typed_data_validation_allows_well_formed_signature_to_dispatch() {
    assert_eq!(
        evaluate_result(permit2_single(), "").unwrap(),
        Verdict::Pass
    );
    assert_eq!(
        evaluate_result(eip2612_permit(), "").unwrap(),
        Verdict::Pass
    );
}

#[derive(Debug)]
struct MalformedAmountAdapter;

impl SignatureActionAdapter for MalformedAmountAdapter {
    fn id(&self) -> ActionAdapterId {
        ActionAdapterId::new("test/malformed-signature-amount@1").unwrap()
    }

    fn match_keys(&self) -> Vec<SignatureMatchKey> {
        vec![SignatureMatchKey::exact(
            1,
            Address::new("0x000000000022d473030f116ddee9f6b43ac78ba3").unwrap(),
            "PermitSingle",
        )]
    }

    fn build_action(&self, sig: &SignatureRequest) -> Result<Action, ActionAdapterError> {
        let token = usdc();
        let approval = Permit2Approval {
            token: token.clone(),
            amount: "not-a-u256".into(),
            expiration: 4600,
            nonce: "1".into(),
        };
        Ok(Action::Permit2(Permit2Action {
            signer: sig.signer.clone(),
            chain_id: sig.chain_id,
            domain_chain_id: sig.typed_data.domain.chain_id,
            verifying_contract: sig.typed_data.domain.verifying_contract.clone(),
            primary_type: sig.typed_data.primary_type.clone(),
            permit_kind: Permit2PermitKind::PermitSingle,
            spender: Address::new("0x1111111111111111111111111111111111111111").unwrap(),
            token,
            amount: approval.amount.clone(),
            expiration: approval.expiration,
            sig_deadline: 1600,
            nonce: approval.nonce.clone(),
            approvals: vec![approval],
            is_unlimited: false,
            nonce_valid: true,
            witness_present: false,
            total_approved_usd: None,
        }))
    }
}

#[derive(Debug)]
struct OwnerMismatchEip2612Adapter;

impl SignatureActionAdapter for OwnerMismatchEip2612Adapter {
    fn id(&self) -> ActionAdapterId {
        ActionAdapterId::new("test/eip2612-owner-mismatch@1").unwrap()
    }

    fn match_keys(&self) -> Vec<SignatureMatchKey> {
        vec![SignatureMatchKey::exact(
            1,
            Address::new(USDC_ADDRESS).unwrap(),
            "Permit",
        )]
    }

    fn build_action(&self, sig: &SignatureRequest) -> Result<Action, ActionAdapterError> {
        Ok(Action::Eip2612(Eip2612Action {
            signer: sig.signer.clone(),
            owner: Address::new("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
            chain_id: sig.chain_id,
            domain_chain_id: sig.typed_data.domain.chain_id,
            verifying_contract: sig.typed_data.domain.verifying_contract.clone(),
            primary_type: sig.typed_data.primary_type.clone(),
            spender: Address::new("0x1111111111111111111111111111111111111111").unwrap(),
            token: usdc(),
            is_unlimited: false,
            nonce_valid: true,
            value: "50000000".into(),
            deadline: 1600,
            nonce: "1".into(),
            total_approved_usd: None,
        }))
    }
}

#[test]
fn malformed_signature_amount_returns_pipeline_error() {
    let clock = MockClock::with_fixed(1000);
    let sig_registry =
        MockSignatureActionAdapterRegistry::new().with_adapter(Arc::new(MalformedAmountAdapter));

    let error =
        evaluate_result_with_registry(permit2_single(), "", &clock, &sig_registry).unwrap_err();

    assert_lowering_error(&error, "not-a-u256");
}

#[test]
fn permit2_human_amount_cap_fires_for_clamped_human_amount() {
    let denied = permit2_set_details("amount", json!(UINT160_MAX));
    assert_fail_id(
        evaluate(denied.clone(), PERMIT2_HUMAN_MAX),
        "user/signature/permit2/max-human-amount-50",
    );

    const CLAMP_ONLY: &str = r#"
@id("test/signature/permit2/clamped-human-amount")
@severity("deny")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context.amountHumanClampedAtCeiling
};
"#;
    assert_fail_id(
        evaluate(denied, CLAMP_ONLY),
        "test/signature/permit2/clamped-human-amount",
    );
}

#[test]
fn permit2_batch_representative_uses_largest_human_amount() {
    const REPRESENTATIVE_USDC: &str = r#"
@id("test/signature/permit2/representative-usdc")
@severity("deny")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context.token.symbol == "USDC"
  && context.amountHuman == decimal("50.0000")
};
"#;

    assert_fail_id(
        evaluate(permit2_batch(), REPRESENTATIVE_USDC),
        "test/signature/permit2/representative-usdc",
    );
}

#[test]
fn eip2612_spender_allowlist_passes_and_fails() {
    assert_eq!(
        evaluate(eip2612_permit(), EIP2612_SPENDER_ALLOWLIST),
        Verdict::Pass
    );
    let denied = eip2612_set_message(
        "spender",
        json!("0x2222222222222222222222222222222222222222"),
    );
    assert_fail_id(
        evaluate(denied, EIP2612_SPENDER_ALLOWLIST),
        "user/signature/eip2612/spender-allowlist",
    );
}

#[test]
fn eip2612_signer_owner_match_permits_policy_evaluation() {
    assert_eq!(
        evaluate(eip2612_permit(), EIP2612_SPENDER_ALLOWLIST),
        Verdict::Pass
    );
}

#[test]
fn eip2612_signer_owner_mismatch_fails_adapter_build() {
    let mut denied = eip2612_permit();
    denied.typed_data.message["owner"] = json!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

    let error =
        evaluate_result(denied, EIP2612_SPENDER_ALLOWLIST).expect_err("adapter must reject owner");

    match error {
        PipelineError::AdapterBuild(message) => {
            assert!(message.contains("message.owner"));
            assert!(message.contains("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
            assert!(message.contains("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        }
        other => panic!("expected PipelineError::AdapterBuild, got {other:?}"),
    }
}

#[test]
fn eip2612_owner_match_policy_remains_defense_in_depth() {
    const OWNER_MATCH: &str = r#"
@id("test/signature/eip2612/owner-match")
@severity("deny")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context.owner != context.base.signer
};
"#;
    let clock = MockClock::with_fixed(1000);
    let sig_registry = MockSignatureActionAdapterRegistry::new()
        .with_adapter(Arc::new(OwnerMismatchEip2612Adapter));

    // Redundant with adapter-level enforcement; kept as belt-and-braces
    // defense in case the adapter is bypassed via Action construction in tests.
    assert_fail_id(
        evaluate_result_with_registry(eip2612_permit(), OWNER_MATCH, &clock, &sig_registry)
            .expect("pipeline ok"),
        "test/signature/eip2612/owner-match",
    );
}

#[test]
fn eip2612_no_unlimited_amount_passes_and_fails() {
    assert_eq!(
        evaluate(eip2612_permit(), EIP2612_NO_UNLIMITED),
        Verdict::Pass
    );
    let denied = eip2612_set_message("value", json!(UINT256_MAX));
    assert_fail_id(
        evaluate(denied, EIP2612_NO_UNLIMITED),
        "user/signature/eip2612/no-unlimited-amount",
    );
}

#[test]
fn eip2612_usd_cap_passes_and_fails() {
    assert_eq!(evaluate(eip2612_permit(), EIP2612_MAX_USD), Verdict::Pass);
    let denied = eip2612_set_message("value", json!("200000000"));
    assert_fail_id(
        evaluate(denied, EIP2612_MAX_USD),
        "user/signature/eip2612/max-usd-100",
    );
}

#[test]
fn eip2612_deadline_window_passes_and_fails() {
    assert_eq!(evaluate(eip2612_permit(), EIP2612_DEADLINE), Verdict::Pass);
    let denied = eip2612_set_message("deadline", json!(5000));
    assert_fail_id(
        evaluate(denied, EIP2612_DEADLINE),
        "user/signature/eip2612/deadline-le-1h",
    );
}

#[test]
fn eip2612_chain_match_passes_and_fails() {
    assert_eq!(evaluate(eip2612_permit(), EIP2612_CHAIN), Verdict::Pass);
    let mut denied = eip2612_permit();
    denied.chain_id = 137;
    assert_fail_id(
        evaluate(denied, EIP2612_CHAIN),
        "user/signature/eip2612/chain-must-match",
    );
}

#[test]
fn eip2612_nonce_sanity_passes_and_fails() {
    assert_eq!(evaluate(eip2612_permit(), EIP2612_NONCE), Verdict::Pass);
    let denied = eip2612_set_message("nonce", json!(UINT256_MAX));
    assert_fail_id(
        evaluate(denied, EIP2612_NONCE),
        "user/signature/eip2612/nonce-sanity",
    );
}

#[test]
fn eip2612_human_value_cap_passes_and_fails() {
    assert_eq!(
        evaluate(
            eip2612_set_message("value", json!("10000000")),
            EIP2612_HUMAN_MAX
        ),
        Verdict::Pass
    );
    let denied = eip2612_set_message("value", json!("100000000"));
    assert_fail_id(
        evaluate(denied, EIP2612_HUMAN_MAX),
        "user/signature/eip2612/max-human-value-50",
    );
}

#[test]
fn eip712_other_verifying_contract_allowlist_passes_and_fails() {
    assert_eq!(
        evaluate(eip712_other(), OTHER_VERIFYING_ALLOWLIST),
        Verdict::Pass
    );
    let mut denied = eip712_other();
    denied.typed_data.domain.verifying_contract =
        Address::new("0x8888888888888888888888888888888888888888").unwrap();
    assert_fail_id(
        evaluate(denied, OTHER_VERIFYING_ALLOWLIST),
        "user/signature/eip712-other/verifying-contract-allowlist",
    );
}

#[test]
fn eip712_other_chain_match_passes_and_fails() {
    assert_eq!(evaluate(eip712_other(), OTHER_CHAIN), Verdict::Pass);
    let mut denied = eip712_other();
    denied.chain_id = 137;
    assert_fail_id(
        evaluate(denied, OTHER_CHAIN),
        "user/signature/eip712-other/chain-must-match",
    );
}

#[test]
fn signature_verdict_is_reproducible_with_fixed_clock() {
    let tx_registry = MockTransactionActionAdapterRegistry::new();
    let sig_registry = default_signature_registry();
    let oracle = oracle();
    let clock = MockClock::with_fixed(1000);
    let policies = PolicyEngine::from_sources([PERMIT2_DEADLINE]).expect("policy source parses");
    let pipe = Pipeline::new(
        &tx_registry,
        HostCapabilities::new(&oracle).with_clock(&clock),
        &policies,
    )
    .with_signature_registry(&sig_registry);
    let sig = permit2_single();

    let first = pipe
        .evaluate(&Request::Sig(sig.clone()))
        .expect("pipeline ok");
    let second = pipe.evaluate(&Request::Sig(sig)).expect("pipeline ok");
    assert_eq!(first, second);
}

#[test]
fn signature_verdict_changes_with_clock() {
    const EXPIRED_PERMIT: &str = r#"
@id("test/signature/eip2612/expired")
@severity("deny")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context.deadlineDeltaSec == 0
};
"#;
    let before_deadline = MockClock::with_fixed(1599);
    let after_deadline = MockClock::with_fixed(1601);

    let before = evaluate_with_clock(eip2612_permit(), EXPIRED_PERMIT, &before_deadline);
    let after = evaluate_with_clock(eip2612_permit(), EXPIRED_PERMIT, &after_deadline);

    assert_eq!(before, Verdict::Pass);
    assert_fail_id(after.clone(), "test/signature/eip2612/expired");
    assert_ne!(before, after);
}

#[test]
fn tx_no_match_stays_transaction_other_not_signature_other() {
    let tx_registry = MockTransactionActionAdapterRegistry::new();
    let sig_registry = default_signature_registry();
    let oracle = oracle();
    let clock = MockClock::with_fixed(1000);
    let policies = PolicyEngine::from_sources([OTHER_VERIFYING_ALLOWLIST]).unwrap();
    let pipe = Pipeline::new(
        &tx_registry,
        HostCapabilities::new(&oracle).with_clock(&clock),
        &policies,
    )
    .with_signature_registry(&sig_registry);

    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        to: Address::new("0x3333333333333333333333333333333333333333").unwrap(),
        value_wei: "0".into(),
        data: vec![0xde, 0xad, 0xbe, 0xef],
        gas: None,
        nonce: None,
    };
    assert_eq!(pipe.evaluate(&Request::Tx(tx)).unwrap(), Verdict::Pass);
}
