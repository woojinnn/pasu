//! End-to-end behavior for the `swap-usdc-amount-cap-deny` bundle: build the
//! REAL `PolicyEngine` from the shipped bundle (policy + its manifest-synthesized
//! schema), then evaluate swap contexts to prove the deny fires only when the
//! enrichment field `context.custom.amountInNano` (populated by the live
//! `token.normalize_to_nano` host call) crosses 0.05 USDC = 50_000_000 nano AND
//! the input token is USDC.
//!
//! This is the post-materialize view: the host has already run the policy-RPC and
//! folded `amountInNano` into `context.custom`, so the context carries it. The
//! manifest/schema consistency + deny-not-fail-open invariants are covered
//! separately by `default_policies_v2.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use policy_engine::policy::PolicyEngine;
use policy_engine::policy_rpc::ManifestV2;
use policy_engine::schema::compose_per_policy;
use serde_json::{json, Value};

const ARBITRUM_USDC: &str = "0xaf88d065e77c8cc2239327c5edb3a432268e5831";
const ARBITRUM_WETH: &str = "0x82af49447d8a07e3bd95bd0d56f35241523fbab1";

fn bundle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/default_policies_v2/swap-usdc-amount-cap-deny")
}

/// Build the real engine from the shipped bundle (schema-less eval path).
fn engine() -> PolicyEngine {
    let dir = bundle_dir();
    let manifest: ManifestV2 =
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    let policy = fs::read_to_string(dir.join("policy.cedar")).unwrap();
    let schema = compose_per_policy(&manifest).expect("compose schema");
    PolicyEngine::build_from_per_policy(&[(policy, schema)]).expect("build engine")
}

/// A swap context with the given input-token address and (optionally) the
/// host-folded `custom.amountInNano`, mirroring what materialization produces.
fn swap_ctx(token_in: &str, nano: Option<i64>) -> Value {
    let mut ctx = json!({
        "tokenIn": { "key": { "standard": "erc20", "chain": "eip155:42161", "address": token_in } },
        "direction": { "kind": "exact_input", "amountIn": "0x22d2a", "minAmountOut": "0x0" }
    });
    if let Some(n) = nano {
        ctx["custom"] = json!({ "amountInNano": n });
    }
    ctx
}

fn is_deny(v: &policy_engine::policy::Verdict) -> bool {
    matches!(v, policy_engine::policy::Verdict::Fail(_))
}

#[test]
fn usdc_swap_at_or_above_5_hundredths_is_denied() {
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

    // Screenshot case: 0.14269 USDC → 142_690_000 nano → DENY.
    assert!(
        is_deny(&eval(swap_ctx(ARBITRUM_USDC, Some(142_690_000)))),
        "0.14269 USDC"
    );
    // Exact boundary: 0.05 USDC = 50_000_000 nano → DENY.
    assert!(
        is_deny(&eval(swap_ctx(ARBITRUM_USDC, Some(50_000_000)))),
        "0.05 USDC"
    );
    // Below cap: 0.04 USDC = 40_000_000 nano → PASS.
    assert!(
        !is_deny(&eval(swap_ctx(ARBITRUM_USDC, Some(40_000_000)))),
        "0.04 USDC"
    );
    // Right token, but enrichment absent → has-guard false → PASS (the required
    // call would fail-CLOSED upstream at materialize; the policy itself is inert).
    assert!(!is_deny(&eval(swap_ctx(ARBITRUM_USDC, None))), "no nano");
    // Big amount but NOT USDC (WETH) → address guard false → PASS.
    assert!(
        !is_deny(&eval(swap_ctx(ARBITRUM_WETH, Some(999_999_999)))),
        "WETH"
    );
}
