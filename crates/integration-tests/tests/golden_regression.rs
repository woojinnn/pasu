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
    let envelopes = route(name).unwrap_or_else(|e| {
        panic!("fixture {name} should route successfully, got error: {e}")
    });
    assert_eq!(envelopes.len(), 1, "fixture {name} expected 1 envelope, got {}", envelopes.len());
    assert_eq!(
        envelopes[0].action.kind(),
        expected_kind,
        "fixture {name} expected action kind {expected_kind}",
    );
}

#[test]
fn swap_uniswap_v2_exact_in_routes() {
    assert_single_action("swap_uniswap_v2_exact_in.json", "swap");
}

#[test]
fn swap_uniswap_v2_exact_out_routes() {
    assert_single_action("swap_uniswap_v2_exact_out.json", "swap");
}

#[test]
fn swap_uniswap_v3_exact_input_single_routes() {
    assert_single_action("swap_uniswap_v3_exact_input_single.json", "swap");
}

#[test]
fn swap_uniswap_v3_exact_input_multi_routes() {
    assert_single_action("swap_uniswap_v3_exact_input_multi.json", "swap");
}

#[test]
fn swap_universal_router_routes() {
    assert_single_action("swap_universal_router.json", "swap");
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
fn erc20_approve_returns_error_for_now() {
    // ERC-20 `approve(spender, amount)` has no Mapper registered yet — once
    // we add an approve Mapper this assertion will need to flip to a
    // structural check on Action::Approve. Until then, NoCallMatch is the
    // expected outcome.
    let result = route("erc20_approve.json");
    assert!(
        result.is_err(),
        "erc20 approve fixture has no mapper yet, should error",
    );
}
