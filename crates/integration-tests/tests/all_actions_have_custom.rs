//! Phase 2 Task 2.2 — every shipped action cedarschema must declare an empty
//! `<Action>CustomContext` placeholder and host a `custom?:` field on its
//! base `Context` type, so manifests can attach their fragments through the
//! composer.

const ACTIONS: &[(&str, &str)] = &[
    // (category_dir, action_name_snake)
    ("DEX", "swap"),
    ("DEX", "add_liquidity"),
    ("DEX", "remove_liquidity"),
    ("DEX", "mint_liquidity_nft"),
    ("DEX", "burn_liquidity_nft"),
    ("DEX", "increase_liquidity"),
    ("DEX", "decrease_liquidity"),
    ("DEX", "initialize_pool"),
    ("DEX", "donate"),
    ("lending", "supply"),
    ("lending", "withdraw"),
    ("lending", "borrow"),
    ("lending", "repay"),
    ("lending", "liquidate"),
    ("lending", "flash_loan"),
    ("lending", "set_authorization"),
    ("lending", "sign_authorization"),
    ("lending", "revoke"),
    ("staking", "stake"),
    ("staking", "request_unstake"),
    ("staking", "claim_unstake"),
    ("restaking", "restake"),
    ("restaking", "request_restake_withdrawal"),
    ("restaking", "claim_restake_withdrawal"),
    ("misc", "wrap"),
    ("misc", "unwrap"),
    ("misc", "approve"),
    ("misc", "set_approval_for_all"),
    ("misc", "transfer"),
    ("misc", "permit"),
    ("misc", "claim_rewards"),
    ("misc", "sign_message"),
    ("misc", "delegate"),
    ("misc", "vote"),
];

fn snake_to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for ch in s.chars() {
        if ch == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

#[test]
fn every_action_cedarschema_declares_custom_context() {
    assert_eq!(ACTIONS.len(), 34, "must cover all 34 shipped actions");
    for (cat, action) in ACTIONS {
        let pascal = snake_to_pascal(action);
        let path = format!("../../schema/policy-schema/actions/{cat}/{action}.cedarschema");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"));
        assert!(
            text.contains(&format!("custom?: {pascal}CustomContext")),
            "{path} must declare `custom?: {pascal}CustomContext` on its context type"
        );
        assert!(
            text.contains(&format!("type {pascal}CustomContext = {{}};")),
            "{path} must declare an empty `{pascal}CustomContext` stub"
        );
    }
}
