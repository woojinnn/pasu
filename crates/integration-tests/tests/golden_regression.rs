//! Golden-vector regression test for the new `request_router::route_request`
//! pipeline. For each input fixture in `data/golden/inputs/`, exercise the
//! pipeline end-to-end and check the structural shape we expect.
//!
//! This is intentionally permissive: it asserts that fixtures we expect to
//! route successfully produce the right `action` kind, and that ones we
//! expect to NOT match return an error. Full byte-exact JSON regression
//! against `baseline_pre_refactor/` is intentionally NOT done here because
//! the new pipeline emits a different (32-variant) Action wire format than
//! the legacy Pipeline that produced the pre-refactor baseline.

use std::fs;
use std::path::PathBuf;

use mappers::EmptyTokenRegistry;
use request_router::{route_request, DefaultRegistries, RouterContext};

fn inputs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/golden/inputs")
}

fn read_fixture(name: &str) -> serde_json::Value {
    let path = inputs_dir().join(name);
    let bytes = fs::read(&path).expect("fixture file present");
    serde_json::from_slice(&bytes).expect("fixture is valid JSON")
}

fn route(name: &str) -> Result<Vec<policy_engine::ActionEnvelope>, request_router::RouterError> {
    let fixture = read_fixture(name);
    let registries = DefaultRegistries::standard();
    let token_registry = EmptyTokenRegistry;
    let ctx = RouterContext {
        registries: &registries,
        token_registry: &token_registry,
        block_timestamp: None,
    };
    route_request(
        &ctx,
        fixture["rpc"]["method"].as_str().expect("method"),
        &fixture["rpc"]["params"],
        fixture["chain_id"].as_u64().expect("chain_id"),
    )
}

fn assert_single_action(name: &str, expected_kind: &str) {
    let envelopes = route(name)
        .unwrap_or_else(|e| panic!("fixture {name} should route successfully, got error: {e}"));
    assert_eq!(
        envelopes.len(),
        1,
        "fixture {name} expected 1 envelope, got {}",
        envelopes.len()
    );
    assert_eq!(
        envelopes[0].action.kind(),
        expected_kind,
        "fixture {name} expected action kind {expected_kind}",
    );
}

fn unwrap_swap(env: &policy_engine::ActionEnvelope) -> &policy_engine::action::dex::SwapAction {
    use policy_engine::action::envelope::Action;
    if let Action::Swap(swap) = &env.action {
        swap
    } else {
        panic!(
            "expected Action::Swap, got kind={} (category={:?})",
            env.action.kind(),
            env.category,
        );
    }
}

#[test]
fn swap_uniswap_v2_exact_in_routes() {
    let envelopes = route("swap_uniswap_v2_exact_in.json").expect("route ok");
    assert_eq!(envelopes.len(), 1);
    let swap = unwrap_swap(&envelopes[0]);
    use policy_engine::action::common::AmountKind;
    use policy_engine::action::dex::SwapMode;
    assert_eq!(swap.mode, SwapMode::ExactIn);
    assert_eq!(swap.amount_in.kind, AmountKind::Exact);
    assert_eq!(swap.amount_out.kind, AmountKind::Min);
}

#[test]
fn swap_uniswap_v2_exact_out_routes() {
    let envelopes = route("swap_uniswap_v2_exact_out.json").expect("route ok");
    assert_eq!(envelopes.len(), 1);
    let swap = unwrap_swap(&envelopes[0]);
    use policy_engine::action::common::AmountKind;
    use policy_engine::action::dex::SwapMode;
    assert_eq!(swap.mode, SwapMode::ExactOut);
    assert_eq!(swap.amount_in.kind, AmountKind::Max);
    assert_eq!(swap.amount_out.kind, AmountKind::Exact);
}

#[test]
fn swap_uniswap_v3_exact_input_single_routes() {
    let envelopes = route("swap_uniswap_v3_exact_input_single.json").expect("route ok");
    assert_eq!(envelopes.len(), 1);
    let swap = unwrap_swap(&envelopes[0]);
    use policy_engine::action::dex::SwapMode;
    assert_eq!(swap.mode, SwapMode::ExactIn);
    assert!(swap.fee_bps.is_some(), "V3 single-hop should carry fee_bps");
}

#[test]
fn swap_uniswap_v3_exact_input_multi_routes() {
    let envelopes = route("swap_uniswap_v3_exact_input_multi.json").expect("route ok");
    assert_eq!(envelopes.len(), 1);
    let swap = unwrap_swap(&envelopes[0]);
    use policy_engine::action::dex::SwapMode;
    assert_eq!(swap.mode, SwapMode::ExactIn);
    assert!(
        swap.fee_bps.is_some(),
        "V3 multi-hop projects to first hop's fee"
    );
    // token_in and token_out must differ on a real multi-hop fixture.
    assert_ne!(
        swap.token_in.address, swap.token_out.address,
        "multi-hop swap collapsed to a single token",
    );
}

#[test]
fn swap_universal_router_routes() {
    let envelopes = route("swap_universal_router.json").expect("route ok");
    assert_eq!(envelopes.len(), 1);
    let swap = unwrap_swap(&envelopes[0]);
    use policy_engine::action::dex::SwapMode;
    assert_eq!(swap.mode, SwapMode::ExactIn);
}

#[test]
fn eip2612_permit_routes() {
    assert_single_action("eip2612_permit.json", "permit");
}

#[test]
fn permit2_permit_single_routes() {
    assert_single_action("permit2_permit_single.json", "permit");
}

#[test]
fn permit2_permit_batch_emits_multiple_envelopes() {
    let envelopes = route("permit2_permit_batch.json")
        .expect("permit2 batch fixture should route successfully");
    assert!(
        envelopes.len() >= 2,
        "expected ≥2 envelopes from a batch permit, got {}",
        envelopes.len(),
    );
    for env in &envelopes {
        assert_eq!(env.action.kind(), "permit");
    }
}

#[test]
fn unknown_selector_returns_error() {
    let result = route("unknown_selector.json");
    assert!(
        result.is_err(),
        "unknown selector fixture should fail to route, got Ok({:?})",
        result.ok(),
    );
}

#[test]
fn erc20_approve_routes_to_unlimited_approve_envelope() {
    use policy_engine::action::common::AmountKind;
    use policy_engine::action::envelope::{Action, Category};
    use policy_engine::action::misc::ApprovalKind;

    let envelopes =
        route("erc20_approve.json").expect("erc20 approve fixture should route via ERC-20 mapper");
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].category, Category::Misc);
    let Action::Approve(approve) = &envelopes[0].action else {
        panic!(
            "expected Action::Approve, got kind={}",
            envelopes[0].action.kind()
        );
    };
    // Fixture uses USDT and ffff…ff amount, so we expect Unlimited.
    assert_eq!(approve.amount.kind, AmountKind::Unlimited);
    assert!(approve.amount.value.is_none());
    assert_eq!(approve.approval_kind, ApprovalKind::Erc20);
}

#[test]
fn erc20_transfer_routes_to_transfer_envelope() {
    use policy_engine::action::envelope::{Action, Category};

    let envelopes = route("erc20_transfer.json")
        .expect("erc20 transfer fixture should route via ERC-20 mapper");
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].category, Category::Misc);
    let Action::Transfer(_transfer) = &envelopes[0].action else {
        panic!(
            "expected Action::Transfer, got kind={}",
            envelopes[0].action.kind()
        );
    };
}
