//! Behavioral check for the two user-authored swap caps:
//!   - `swap-min-usd-cap-deny`     — deny when input USD value ≥ $0.05
//!   - `swap-min-intoken-cap-deny` — deny when input token amount ≥ 0.05 token
//!
//! Mirrors the engine's schema-less authorize path (baseline permit + the
//! forbid), so a fired forbid flips the decision Allow → Deny exactly as the
//! shipped PolicyEngine would.

use std::str::FromStr;

use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request};

const USD_POLICY: &str = include_str!("fixtures/default_policies_v2/swap-min-usd-cap-deny.cedar");
const INTOKEN_POLICY: &str =
    include_str!("fixtures/default_policies_v2/swap-min-intoken-cap-deny.cedar");
const USDC_POLICY: &str = include_str!("fixtures/default_policies_v2/swap-usdc-input-deny.cedar");

/// Authorize a Swap with the given policy + context, against a baseline permit.
fn decide(policy_text: &str, context_json: serde_json::Value) -> Decision {
    let combined = format!("permit(principal, action, resource);\n{policy_text}");
    let pset = PolicySet::from_str(&combined).expect("policy parses");

    let principal: EntityUid = r#"Wallet::"0xwallet""#.parse().unwrap();
    let action: EntityUid = r#"Amm::Action::"Swap""#.parse().unwrap();
    let resource: EntityUid = r#"Protocol::"uniswap""#.parse().unwrap();

    let context = Context::from_json_value(context_json, None).expect("context parses");
    let request = Request::new(principal, action, resource, context, None).expect("request");

    Authorizer::new()
        .is_authorized(&request, &pset, &Entities::empty())
        .decision()
}

fn usd_ctx(arg: &str) -> serde_json::Value {
    serde_json::json!({
        "direction": { "amountInUsd": { "__extn": { "fn": "decimal", "arg": arg } } }
    })
}

fn intoken_ctx(nano: i64) -> serde_json::Value {
    serde_json::json!({ "direction": { "amountInNano": nano } })
}

/// Build a swap context whose input token is the given erc20 address (lowercase
/// 0x-hex, exactly as the lowering emits it).
fn tokenin_ctx(address: &str) -> serde_json::Value {
    serde_json::json!({
        "tokenIn": { "key": { "standard": "erc20", "chain": "eip155:1", "address": address } }
    })
}

#[test]
fn usd_cap_denies_at_or_above_5_cents() {
    // The picture: 0.14269 USDC ≈ $0.11 input → blocked.
    assert_eq!(
        decide(USD_POLICY, usd_ctx("0.1100")),
        Decision::Deny,
        "$0.11"
    );
    // Exact boundary ($0.05) is blocked (greaterThanOrEqual / "이상").
    assert_eq!(
        decide(USD_POLICY, usd_ctx("0.0500")),
        Decision::Deny,
        "$0.05"
    );
    // Below the cap passes.
    assert_eq!(
        decide(USD_POLICY, usd_ctx("0.0400")),
        Decision::Allow,
        "$0.04"
    );
    // Field absent → has-guard false → fail-open (Allow).
    assert_eq!(
        decide(USD_POLICY, serde_json::json!({ "direction": {} })),
        Decision::Allow,
        "absent amountInUsd"
    );
}

#[test]
fn usdc_input_swap_is_denied_by_address() {
    // The screenshot: selling Arbitrum USDC → blocked (real, populated field).
    assert_eq!(
        decide(
            USDC_POLICY,
            tokenin_ctx("0xaf88d065e77c8cc2239327c5edb3a432268e5831")
        ),
        Decision::Deny,
        "Arbitrum USDC in"
    );
    // Ethereum mainnet USDC → blocked.
    assert_eq!(
        decide(
            USDC_POLICY,
            tokenin_ctx("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        ),
        Decision::Deny,
        "Ethereum USDC in"
    );
    // WETH (not USDC) → passes.
    assert_eq!(
        decide(
            USDC_POLICY,
            tokenin_ctx("0x82af49447d8a07e3bd95bd0d56f35241523fbab1")
        ),
        Decision::Allow,
        "WETH in"
    );
    // Native gas token (no address) → has-guard false → passes.
    assert_eq!(
        decide(
            USDC_POLICY,
            serde_json::json!({ "tokenIn": { "key": { "standard": "native", "chain": "eip155:1" } } })
        ),
        Decision::Allow,
        "native in"
    );
}

#[test]
fn intoken_cap_denies_at_or_above_5_hundredths_token() {
    // 0.05 token = 50_000_000 nano (boundary) → blocked.
    assert_eq!(
        decide(INTOKEN_POLICY, intoken_ctx(50_000_000)),
        Decision::Deny,
        "0.05 tok"
    );
    // 0.1 token = 100_000_000 nano → blocked.
    assert_eq!(
        decide(INTOKEN_POLICY, intoken_ctx(100_000_000)),
        Decision::Deny,
        "0.1 tok"
    );
    // 0.04 token = 40_000_000 nano → passes.
    assert_eq!(
        decide(INTOKEN_POLICY, intoken_ctx(40_000_000)),
        Decision::Allow,
        "0.04 tok"
    );
    // Field absent → fail-open (Allow).
    assert_eq!(
        decide(INTOKEN_POLICY, serde_json::json!({ "direction": {} })),
        Decision::Allow,
        "absent amountInNano"
    );
}
