//! Phase 0 de-risk: prove Cedar EST round-trip is lossless + emit the EST
//! corpus fixtures consumed by the dashboard TS test suite.
//!
//! See docs/superpowers/plans/2026-06-02-cedar-block-ir-conversion.md (Task 0).

use cedar_policy::Policy;
use policy_engine_wasm::{est_json_to_policy_text, policy_text_to_est_json};
use std::str::FromStr;

/// (name, category, cedar_text) — covers the 12 spec test categories.
const CORPUS: &[(&str, &str, &str)] = &[
    (
        "nesting_bool",
        "recursion",
        r#"permit(principal, action, resource) when { ((context.a && context.b) || (context.c && context.d)) && (context.e || context.f) };"#,
    ),
    (
        "chained_attr",
        "recursion",
        r#"permit(principal, action, resource) when { context.a.b.c.d == 1 };"#,
    ),
    (
        "assoc_sub",
        "recursion",
        r#"permit(principal, action, resource) when { context.a - context.b - context.c == 0 };"#,
    ),
    (
        "repeated_field",
        "same-name",
        r#"permit(principal, action, resource) when { (context.custom.inputAmount > 30 && context.custom.inputAmount < 60) || (context.custom.inputAmount > 90 && context.custom.inputAmount < 120) };"#,
    ),
    (
        "repeated_dup",
        "same-name",
        r#"permit(principal, action, resource) when { context.x > 30 || context.x > 30 };"#,
    ),
    (
        "precedence",
        "precedence",
        r#"permit(principal, action, resource) when { context.a || context.b && context.c };"#,
    ),
    (
        "neg_vs_sub",
        "precedence",
        r#"permit(principal, action, resource) when { context.a - 5 == -5 };"#,
    ),
    (
        "has_single",
        "operators",
        r#"permit(principal, action, resource) when { context has x };"#,
    ),
    (
        "has_path",
        "operators",
        r#"permit(principal, action, resource) when { context has a.b.c };"#,
    ),
    (
        "like_escape",
        "operators",
        r#"permit(principal, action, resource) when { resource.name like "foo*bar\*baz" };"#,
    ),
    (
        "is_in",
        "operators",
        r#"permit(principal is User in Group::"admins", action, resource);"#,
    ),
    (
        "contains_ops",
        "operators",
        r#"permit(principal, action, resource) when { context.tags.contains("x") && [1,2,3].containsAll([1,2]) };"#,
    ),
    (
        "if_then_else",
        "operators",
        r#"permit(principal, action, resource) when { (if context.a then 1 else 2) == 1 };"#,
    ),
    (
        "ext_decimal",
        "ext",
        r#"permit(principal, action, resource) when { context.rate.lessThan(decimal("0.10")) };"#,
    ),
    (
        "ext_ip",
        "ext",
        r#"permit(principal, action, resource) when { context.src.isInRange(ip("10.0.0.0/24")) };"#,
    ),
    (
        "literals",
        "literals",
        r#"permit(principal, action, resource) when { context.n == 9223372036854775807 && context.s == "a\"b\\c" && context.set == [] && context.rec == {} };"#,
    ),
    (
        "entity_uid",
        "literals",
        r#"permit(principal == My::Name::Space::User::"al ice", action, resource);"#,
    ),
    (
        "scope_forbid",
        "scope",
        r#"forbid(principal, action == Action::"Swap", resource is Vault);"#,
    ),
    (
        "action_set",
        "scope",
        r#"permit(principal, action in [Action::"a", Action::"b"], resource);"#,
    ),
    (
        "annotations",
        "annotations",
        "@id(\"p1\") @severity(\"warn\")\npermit(principal, action, resource);",
    ),
    (
        "when_unless",
        "conditions",
        r#"permit(principal, action, resource) when { context.a } unless { context.b };"#,
    ),
    (
        "no_conditions",
        "conditions",
        r#"permit(principal, action, resource);"#,
    ),
    (
        "expr_is",
        "operators",
        r#"permit(principal, action, resource) when { resource is Vault };"#,
    ),
    (
        "unary_not",
        "operators",
        r#"permit(principal, action, resource) when { !(context.flag) };"#,
    ),
    (
        "is_empty",
        "operators",
        r#"permit(principal, action, resource) when { context.tags.isEmpty() };"#,
    ),
    (
        "has_tag",
        "operators",
        r#"permit(principal, action, resource) when { resource.hasTag("admin") };"#,
    ),
    (
        "get_tag",
        "operators",
        r#"permit(principal, action, resource) when { resource.getTag("level") == 1 };"#,
    ),
    (
        "entity_literal",
        "literals",
        r#"permit(principal, action, resource) when { context.owner == User::"alice" };"#,
    ),
];

/// THE load-bearing invariant for the TS EST↔IR engine: an EST survives a
/// `Policy` round-trip (`from_json → to_json`) unchanged. If this holds, AST
/// desugaring does NOT leak into the EST, so our pure EST↔IR layer is faithful.
#[test]
fn est_is_a_faithful_fixed_point() {
    for (name, _cat, text) in CORPUS {
        let est = Policy::from_str(text)
            .unwrap_or_else(|e| panic!("{name}: parse failed: {e}"))
            .to_json()
            .unwrap_or_else(|e| panic!("{name}: to_json failed: {e}"));
        let est2 = Policy::from_json(None, est.clone())
            .unwrap_or_else(|e| panic!("{name}: from_json failed: {e}"))
            .to_json()
            .unwrap();
        assert_eq!(
            est, est2,
            "{name}: EST not a fixed point under from_json→to_json"
        );
    }
}

/// Characterize the text boundary: `to_cedar` renders from the AST and may
/// desugar surface operators (e.g. `>` into `!(_ <= _)`). It is still a stable
/// fixed point — re-parse + re-render is byte-identical — which is all the
/// block→text path needs (semantics preserved; surface form canonicalized).
#[test]
fn to_cedar_text_is_idempotent() {
    for (name, _cat, text) in CORPUS {
        let est = Policy::from_str(text).unwrap().to_json().unwrap();
        let t1 = Policy::from_json(None, est)
            .unwrap()
            .to_cedar()
            .unwrap_or_else(|| panic!("{name}: to_cedar returned None"));
        let est_b = Policy::from_str(&t1).unwrap().to_json().unwrap();
        let t2 = Policy::from_json(None, est_b).unwrap().to_cedar().unwrap();
        assert_eq!(t1, t2, "{name}: to_cedar not idempotent");
    }
}

#[test]
fn emit_est_corpus_fixture() {
    let mut out = Vec::new();
    for (name, cat, text) in CORPUS {
        let est = Policy::from_str(text)
            .unwrap_or_else(|e| panic!("{name}: parse failed: {e}"))
            .to_json()
            .unwrap();
        out.push(serde_json::json!({ "name": name, "category": cat, "text": text, "est": est }));
    }
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../browser-extension/dashboard/src/cedar/blocks/__tests__/fixtures/est-corpus.json"
    );
    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(&out).unwrap()).unwrap();
}

// ── Phase 1: text↔EST WASM exports ──────────────────────────────────────

#[test]
fn text_to_est_ok_and_err() {
    let ok = policy_text_to_est_json("permit(principal, action, resource);".into());
    let v: serde_json::Value = serde_json::from_str(&ok).unwrap();
    assert_eq!(v["ok"], serde_json::json!(true));
    assert_eq!(
        v["policies"][0]["est"]["effect"],
        serde_json::json!("permit")
    );

    let err = policy_text_to_est_json("permit(".into());
    let e: serde_json::Value = serde_json::from_str(&err).unwrap();
    assert_eq!(e["ok"], serde_json::json!(false));
    assert!(!e["error"].as_str().unwrap().is_empty());
}

#[test]
fn est_to_text_ok_and_err() {
    // Round-trip through both exports: text → est → text.
    let est_resp = policy_text_to_est_json("permit(principal, action, resource);".into());
    let est =
        serde_json::from_str::<serde_json::Value>(&est_resp).unwrap()["policies"][0]["est"].clone();
    let resp = est_json_to_policy_text(serde_json::to_string(&est).unwrap());
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["ok"], serde_json::json!(true));
    assert!(v["text"].as_str().unwrap().contains("permit"));

    let bad = est_json_to_policy_text("{\"effect\":\"nope\"}".into());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&bad).unwrap()["ok"],
        serde_json::json!(false)
    );
}
