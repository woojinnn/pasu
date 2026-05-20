//! Phase 4 / Task 4.1: assert the v0 swap example policies live under
//! `policy-rpc/examples/policies/swap/` (their new home) and that the
//! manifest-paired `.cedar` bodies reference `context.custom.<field>` with
//! `has` guards.

#[test]
fn legacy_swap_examples_live_under_policy_rpc_examples() {
    for name in [
        "max-input-usd-100",
        "max-input-usd-3",
        "min-output-usd-floor",
    ] {
        let cedar = format!("../../policy-rpc/examples/policies/swap/{name}.cedar");
        let manifest = format!("../../policy-rpc/examples/policies/swap/{name}.policy-rpc.json");
        assert!(
            std::path::Path::new(&cedar).exists(),
            "expected cedar at {cedar}"
        );
        assert!(
            std::path::Path::new(&manifest).exists(),
            "expected manifest at {manifest}"
        );
        let body = std::fs::read_to_string(&cedar).unwrap();
        assert!(
            body.contains("context.custom."),
            "{cedar} does not reference context.custom.*"
        );
        assert!(body.contains("has "), "{cedar} is missing `has` guards");
    }
}
