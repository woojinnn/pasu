//! Two-ended check for the `swap-input-usd-cap-deny` bundle:
//!   1. its manifest selectors resolve against a real lowered swap context
//!      (`$.action.tokenIn.key.address`, `$.action.direction.amountIn`,
//!      `$.root.chain_id`) — the part that silently breaks if the selector root
//!      or a nested field name is wrong;
//!   2. the policy denies once the host-served `oracle.usd_value` result has been
//!      folded into `context.custom.inputUsd` (the post-materialize view).
//!
//! The server-side computation of `{usd}` from synced price is covered by the
//! `policy-server` handler tests; manifest/schema consistency + deny-not-fail-open
//! by `default_policies_v2.rs`; the materialize plumbing by `materialize_v2.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use policy_engine::policy::{PolicyEngine, Verdict};
use policy_engine::policy_rpc::{resolve_selector, ManifestV2};
use policy_engine::schema::compose_per_policy;
use serde_json::{json, Value};

fn bundle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/default_policies_v2/swap-input-usd-cap-deny")
}

/// The shape `lower_action` emits for an exact-input USDC→WETH swap — only the
/// fields the manifest selectors read.
fn lowered_swap_context() -> Value {
    json!({
        "tokenIn": { "key": { "standard": "erc20", "chain": "eip155:1",
                              "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" } },
        "direction": { "kind": "exact_input", "amountIn": "0x5f5e100", "minAmountOut": "0x0" },
        "recipient": "0x7ee04c7057ad92b7dcc8e9bb26358c6b0a62822c"
    })
}

#[test]
fn manifest_selectors_resolve_against_lowered_swap() {
    let manifest: ManifestV2 =
        serde_json::from_str(&fs::read_to_string(bundle_dir().join("manifest.json")).unwrap())
            .unwrap();
    let action = lowered_swap_context();
    let root = json!({ "chain_id": "eip155:1" });
    let empty = json!({});

    let params = &manifest.policy_rpc[0].params;
    // Resolve each selector exactly as planning does: root→$.root, lowered→$.action.
    let resolve = |sel: &Value| {
        resolve_selector(
            sel.as_str().unwrap(),
            &root,
            &action,
            &empty,
            &empty,
            &empty,
        )
        .unwrap()
    };

    assert_eq!(resolve(&params["chain_id"]), json!("eip155:1"));
    assert_eq!(
        resolve(&params["asset"]),
        json!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
    );
    assert_eq!(resolve(&params["amount"]), json!("0x5f5e100"));
}

fn engine() -> PolicyEngine {
    let dir = bundle_dir();
    let manifest: ManifestV2 =
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    let policy = fs::read_to_string(dir.join("policy.cedar")).unwrap();
    let schema = compose_per_policy(&manifest).expect("compose schema");
    PolicyEngine::build_from_per_policy(&[(policy, schema)]).expect("build engine")
}

fn ctx_with_usd(usd: Option<&str>) -> Value {
    let mut ctx = json!({
        "tokenIn": { "key": { "standard": "erc20", "chain": "eip155:1",
                              "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" } },
        "direction": { "kind": "exact_input", "amountIn": "0x5f5e100", "minAmountOut": "0x0" }
    });
    if let Some(u) = usd {
        ctx["custom"] = json!({ "inputUsd": { "__extn": { "fn": "decimal", "arg": u } } });
    }
    ctx
}

#[test]
fn policy_denies_when_input_usd_at_or_above_5_cents() {
    let eng = engine();
    let entities = json!([]);
    let eval = |ctx: Value| {
        eng.evaluate(
            r#"Wallet::"0x7ee04c7057ad92b7dcc8e9bb26358c6b0a62822c""#,
            r#"Amm::Action::"Swap""#,
            r#"Protocol::"0x4c82d1fbfe28c977cbb58d8c7ff8fcf9f70a2cca""#,
            &entities,
            &ctx,
        )
        .expect("evaluate")
    };
    let is_deny = |v: &Verdict| matches!(v, Verdict::Fail(_));

    assert!(is_deny(&eval(ctx_with_usd(Some("0.1100")))), "$0.11");
    assert!(
        is_deny(&eval(ctx_with_usd(Some("0.0500")))),
        "$0.05 boundary"
    );
    assert!(!is_deny(&eval(ctx_with_usd(Some("0.0400")))), "$0.04");
    assert!(!is_deny(&eval(ctx_with_usd(None))), "no inputUsd");
}
