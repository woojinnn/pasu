//! Round-trip: compile a `PolicyRule` to Cedar text and confirm
//! `policy-engine`'s installer accepts it against the bundled schema.
//!
//! This is the strongest guarantee the generator can give without standing
//! up the full pipeline: if `PolicyEngineBuilder::new().add_text(text).build()`
//! succeeds, the text parses, validates against the bundled `core + swap`
//! schema, and carries the `@id`/`@severity` annotations the runtime needs.

use policy_builder::{
    compile,
    schemas::swap,
    types::{PolicyRule, Predicate, PredicateValue, Severity},
};
use policy_engine::policy::PolicyEngineBuilder;

#[test]
fn unconditional_forbid_installs_in_policy_engine() {
    let rule = PolicyRule {
        id: "user/block-all-swaps".into(),
        action: "swap".into(),
        severity: Severity::Deny,
        reason: "swaps are blocked".into(),
        predicates: vec![],
    };
    let text = compile(&rule, &swap::schema()).expect("compile");
    let result = PolicyEngineBuilder::new().add_text(text.clone()).build();
    assert!(
        result.is_ok(),
        "generated text failed to install:\n----\n{text}\n----\nerror: {:?}",
        result.err()
    );
}

#[test]
fn long_comparison_installs() {
    let rule = PolicyRule {
        id: "user/max-fee-bps-100".into(),
        action: "swap".into(),
        severity: Severity::Deny,
        reason: "fee exceeds 100 bps".into(),
        predicates: vec![Predicate {
            field: "feeBps".into(),
            op: "gt".into(),
            value: PredicateValue::Single("100".into()),
        }],
    };
    let text = compile(&rule, &swap::schema()).expect("compile");
    let result = PolicyEngineBuilder::new().add_text(text.clone()).build();
    assert!(
        result.is_ok(),
        "generated text failed to install:\n----\n{text}\n----\nerror: {:?}",
        result.err()
    );
}

#[test]
fn decimal_with_optional_parent_installs() {
    let rule = PolicyRule {
        id: "user/max-input-usd-100".into(),
        action: "swap".into(),
        severity: Severity::Warn,
        reason: "input exceeds 100 USD".into(),
        predicates: vec![
            Predicate {
                field: "totalInputUsd.value".into(),
                op: "gt".into(),
                value: PredicateValue::Single("100.00".into()),
            },
            Predicate {
                field: "totalInputUsd.staleSec".into(),
                op: "lte".into(),
                value: PredicateValue::Single("60".into()),
            },
        ],
    };
    let text = compile(&rule, &swap::schema()).expect("compile");
    // totalInputUsd is a custom (manifest-enriched) field. The bundled
    // schema's SwapCustomContext is `{}`, so the only access path Cedar can
    // type-check is via `context has custom && context.custom has X` — both
    // guards short-circuit on an absent attribute, so the policy parses and
    // installs even though the value comparison can never fire under the
    // empty custom shape.
    let result = PolicyEngineBuilder::new().add_text(text.clone()).build();
    assert!(
        result.is_ok(),
        "generated text failed to install:\n----\n{text}\n----\nerror: {:?}",
        result.err()
    );
}

#[test]
fn token_record_predicate_installs() {
    // "block any swap whose input token symbol is not USDC"
    let rule = PolicyRule {
        id: "user/usdc-only-input".into(),
        action: "swap".into(),
        severity: Severity::Deny,
        reason: "non-USDC input not allowed".into(),
        predicates: vec![Predicate {
            field: "inputToken.asset.symbol".into(),
            op: "ne".into(),
            value: PredicateValue::Single("USDC".into()),
        }],
    };
    let text = compile(&rule, &swap::schema()).expect("compile");
    let result = PolicyEngineBuilder::new().add_text(text.clone()).build();
    assert!(
        result.is_ok(),
        "generated text failed to install:\n----\n{text}\n----\nerror: {:?}",
        result.err()
    );
}

#[test]
fn token_address_membership_installs() {
    let rule = PolicyRule {
        id: "user/blocklist-rugpull".into(),
        action: "swap".into(),
        severity: Severity::Deny,
        reason: "output token is on the blocklist".into(),
        predicates: vec![Predicate {
            field: "outputToken.asset.address".into(),
            op: "in".into(),
            // Synthetic but well-formed: 40 hex chars after the 0x prefix.
            // The pattern check (^0x[0-9a-fA-F]{40}$) is on by default
            // now, so the literal must be valid hex to compile.
            value: PredicateValue::Multi(vec![
                "0xdeadbeef0000000000000000000000000000dead".into(),
            ]),
        }],
    };
    let text = compile(&rule, &swap::schema()).expect("compile");
    let result = PolicyEngineBuilder::new().add_text(text.clone()).build();
    assert!(
        result.is_ok(),
        "generated text failed to install:\n----\n{text}\n----\nerror: {:?}",
        result.err()
    );
}

#[test]
fn usd_valuation_sources_predicate_installs() {
    // "warn when totalInputUsd sources does not include Chainlink"
    let rule = PolicyRule {
        id: "user/chainlink-required".into(),
        action: "swap".into(),
        severity: Severity::Warn,
        reason: "oracle valuation missing Chainlink".into(),
        predicates: vec![Predicate {
            field: "totalInputUsd.sources".into(),
            op: "contains".into(),
            value: PredicateValue::Single("chainlink".into()),
        }],
    };
    let text = compile(&rule, &swap::schema()).expect("compile");
    let result = PolicyEngineBuilder::new().add_text(text.clone()).build();
    assert!(
        result.is_ok(),
        "generated text failed to install:\n----\n{text}\n----\nerror: {:?}",
        result.err()
    );
}

#[test]
fn bool_predicate_installs() {
    let rule = PolicyRule {
        id: "user/block-contract-recipient".into(),
        action: "swap".into(),
        severity: Severity::Warn,
        reason: "recipient is a contract".into(),
        predicates: vec![Predicate {
            field: "recipientIsContract".into(),
            op: "isTrue".into(),
            value: PredicateValue::None,
        }],
    };
    let text = compile(&rule, &swap::schema()).expect("compile");
    let result = PolicyEngineBuilder::new().add_text(text.clone()).build();
    assert!(
        result.is_ok(),
        "generated text failed to install:\n----\n{text}\n----\nerror: {:?}",
        result.err()
    );
}
