//! Behavioral check for the shipped default `sweep-recipient-not-self-warn`
//! (H2 fund-egress redirect guard). A Uniswap router `sweepToken`/`unwrapWETH9`
//! decodes to `Token::Erc20Transfer` carrying `is_router_egress = true`; this
//! policy warns when its `recipient` is NOT the signer's wallet — and stays
//! dormant for a NORMAL transfer (flag absent), so it never warns on ordinary
//! user sends (no alarm fatigue).
//!
//! Mirrors the engine's schema-less authorize path (baseline permit + the
//! forbid), so a fired forbid flips Allow → Deny exactly as the shipped
//! warn-severity PolicyEngine would surface a warn.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use cedar_policy::{
    Authorizer, Context, Decision, Entities, Entity, EntityUid, PolicySet, Request,
    RestrictedExpression,
};

// The exact forbid from
// `browser-extension/default-bundles/day1-safety/policies/sweep-recipient-not-self-warn/policy.cedar`.
const SWEEP_POLICY: &str = r#"
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has is_router_egress &&
  context.is_router_egress &&
  context.recipient != principal.address
};
"#;

const SIGNER: &str = "0x000000000000000000000000000000000000aaaa";

/// Authorize an `Erc20Transfer` with the given context against a baseline permit,
/// with `principal.address == SIGNER`.
fn decide(context_json: serde_json::Value) -> Decision {
    let combined = format!("permit(principal, action, resource);\n{SWEEP_POLICY}");
    let pset = PolicySet::from_str(&combined).expect("policy parses");

    let principal: EntityUid = format!(r#"Wallet::"{SIGNER}""#).parse().unwrap();
    let action: EntityUid = r#"Token::Action::"Erc20Transfer""#.parse().unwrap();
    let resource: EntityUid = r#"Protocol::"router""#.parse().unwrap();

    // The Wallet entity must carry the `address` attribute the policy reads.
    let mut attrs = HashMap::new();
    attrs.insert(
        "address".to_string(),
        RestrictedExpression::new_string(SIGNER.to_string()),
    );
    let wallet = Entity::new(principal.clone(), attrs, HashSet::new()).expect("wallet entity");
    let entities = Entities::from_entities([wallet], None).expect("entities");

    let context = Context::from_json_value(context_json, None).expect("context parses");
    let request = Request::new(principal, action, resource, context, None).expect("request");

    Authorizer::new()
        .is_authorized(&request, &pset, &entities)
        .decision()
}

#[test]
fn warns_on_router_egress_to_a_non_signer_recipient() {
    // sweepToken(recipient = attacker) hidden in a swap multicall → the redirect
    // is now policy-visible and fires the warn.
    assert_eq!(
        decide(serde_json::json!({
            "is_router_egress": true,
            "recipient": "0x000000000000000000000000000000000000dead"
        })),
        Decision::Deny,
        "router egress to a non-signer recipient must fire"
    );
}

#[test]
fn passes_router_egress_to_the_signer_itself() {
    // A normal ETH-output swap unwraps/sweeps to the user → recipient == signer →
    // no warn (the common, benign case).
    assert_eq!(
        decide(serde_json::json!({
            "is_router_egress": true,
            "recipient": SIGNER
        })),
        Decision::Allow,
        "egress to the signer itself must pass"
    );
}

#[test]
fn never_touches_a_normal_user_transfer() {
    // A normal `transfer(recipient, amount)` to someone else leaves the flag
    // ABSENT → the has-guard is false → no warn (no alarm fatigue on ordinary sends).
    assert_eq!(
        decide(serde_json::json!({
            "recipient": "0x000000000000000000000000000000000000beef"
        })),
        Decision::Allow,
        "a normal transfer (no is_router_egress) must never fire"
    );
    // Even an explicit `false` flag must not fire.
    assert_eq!(
        decide(serde_json::json!({
            "is_router_egress": false,
            "recipient": "0x000000000000000000000000000000000000beef"
        })),
        Decision::Allow,
        "is_router_egress=false must never fire"
    );
}
