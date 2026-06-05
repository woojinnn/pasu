// Native round-trip over the text↔EST wasm exports (no browser bridge).
// policy_text_to_est_json: text -> { ok, policies: [{ id, est }] }
// est_json_to_policy_text: est-json -> { ok, text }
use policy_engine_wasm::{est_json_to_policy_text, policy_text_to_est_json};
use serde_json::Value;

const POLICY: &str =
    r#"permit(principal, action == Action::"Swap", resource) when { context.slippageBp <= 50 };"#;

#[test]
fn text_to_est_to_text_is_stable() {
    // text -> EST
    let out: Value = serde_json::from_str(&policy_text_to_est_json(POLICY.to_string())).unwrap();
    assert_eq!(out["ok"], Value::Bool(true), "to-est failed: {out}");
    let est = out["policies"][0]["est"].clone();
    assert!(est.is_object(), "expected an EST object, got {est}");

    // EST -> text
    let back: Value = serde_json::from_str(&est_json_to_policy_text(
        serde_json::to_string(&est).unwrap(),
    ))
    .unwrap();
    assert_eq!(back["ok"], Value::Bool(true), "to-text failed: {back}");
    let text1 = back["text"].as_str().unwrap().to_string();

    // text -> EST -> text again; second render must be byte-identical (idempotent).
    let out2: Value = serde_json::from_str(&policy_text_to_est_json(text1.clone())).unwrap();
    let est2 = out2["policies"][0]["est"].clone();
    let back2: Value = serde_json::from_str(&est_json_to_policy_text(
        serde_json::to_string(&est2).unwrap(),
    ))
    .unwrap();
    assert_eq!(
        text1,
        back2["text"].as_str().unwrap(),
        "second render diverged"
    );
}

#[test]
fn malformed_text_reports_error_not_panic() {
    let out: Value = serde_json::from_str(&policy_text_to_est_json("permit(".to_string())).unwrap();
    assert_eq!(out["ok"], Value::Bool(false));
    assert!(out["error"].is_string());
}
