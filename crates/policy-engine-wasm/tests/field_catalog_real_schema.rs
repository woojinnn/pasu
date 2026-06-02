//! Walker rigor on the REAL shipped schema. Installing with no custom
//! manifests makes `state.schema_text` the full bundled base schema — every
//! shipped action, real namespaces, cross-namespace common types
//! (`Amm::SwapContext` → `Core::ActionMeta`), and deeply nested records. The
//! toy-schema unit tests in `field_catalog.rs` don't exercise this; this file
//! proves `field_catalog_json` resolves the real complexity without panic.
use policy_engine_wasm::{field_catalog_json, install_policies_json};
use serde_json::{json, Value};

/// Install the bundled base schema (no custom manifests) and return the parsed
/// `field_catalog_json` envelope.
fn install_base_then_catalog() -> Value {
    let out = install_policies_json(
        json!({
            "schema_text": "",
            "manifests": {},
            "policy_set": [{ "id": "b", "text": "permit(principal, action, resource);" }]
        })
        .to_string(),
    );
    let p: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(p["ok"], true, "install failed: {p}");
    let cat: Value = serde_json::from_str(&field_catalog_json()).unwrap();
    assert_eq!(cat["ok"], true, "{cat}");
    cat
}

#[test]
fn covers_shipped_actions_without_panic() {
    let cat = install_base_then_catalog();
    let data = cat["data"].as_object().expect("catalog object");
    // The shipped schema defines dozens of actions across many namespaces.
    assert!(
        data.len() >= 50,
        "expected many shipped actions, got {}",
        data.len()
    );
    for action in [
        "Swap",
        "Borrow",
        "HlOrder",
        "Erc20Approve",
        "OpenPosition",
        "AddLiquidity",
    ] {
        assert!(
            data.contains_key(action),
            "missing shipped action {action} (have {} actions)",
            data.len()
        );
    }
}

#[test]
fn resolves_deep_cross_namespace_typed_leaves_on_real_schema() {
    let cat = install_base_then_catalog();
    let data = cat["data"].as_object().unwrap();
    let swap = data["Swap"].as_array().expect("Swap fields");

    let field = |path: &str| swap.iter().find(|f| f["path"] == path);
    let assert_typed = |path: &str, ty: &str, kind: &str, src: &str| {
        let f = field(path).unwrap_or_else(|| panic!("missing path {path} on Swap"));
        assert_eq!(f["type"], ty, "{path} type");
        assert_eq!(f["fieldKind"], kind, "{path} fieldKind");
        assert_eq!(f["source"], src, "{path} source");
    };

    // Cross-namespace common type resolution (Amm::SwapContext -> Core::ActionMeta).
    assert_typed("context.meta.submittedAt", "Long", "primitive", "base");
    assert_typed("context.meta.submitter", "String", "primitive", "base");
    // 4 levels deep: meta -> nature (ActionNature) -> domain (record) -> chainId.
    assert_typed(
        "context.meta.nature.domain.chainId",
        "Long",
        "primitive",
        "base",
    );
    assert_typed("context.meta.nature.kind", "String", "primitive", "base");
    // Core::TokenRef nested record.
    assert_typed("context.tokenIn.key.address", "String", "primitive", "base");
    // Venue record, mixed primitives.
    assert_typed("context.venue.name", "String", "primitive", "base");
    assert_typed("context.venue.isMeta", "Boolean", "primitive", "base");
    assert_typed("context.slippageBp", "Long", "primitive", "base");
    // Extension type carried through.
    assert_typed(
        "context.direction.amountInUsd",
        "decimal",
        "extension",
        "base",
    );
    // Entity-shape resolution (principal: Wallet -> address).
    assert_typed("principal.address", "String", "primitive", "base");
    // The custom subtree is tagged `custom` even when empty (no manifest installed).
    assert_eq!(
        field("context.custom").expect("context.custom")["source"],
        "custom"
    );
}
