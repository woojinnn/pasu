//! E2E: a Hyperliquid `/exchange` action is evaluated through the **literal
//! extension entry point** — `evaluate_action_v2_json` — using the thin
//! `ActionBody::HyperliquidCore` model.
//!
//! This feeds the EXACT JSON envelope the browser extension's service worker
//! sends — `{ action, meta, tx, bundles, results }` — into
//! `evaluate_action_v2_json(input_json) -> String` and parses the returned
//! `{ ok, data: { verdict } }`. The `action` JSON is byte-for-byte the shape the
//! TS converter (`hl-order-to-action.ts`) emits, so a serde drift on either side
//! fails loudly here instead of silently fail-closing at runtime.
//!
//! Run: `cargo test -p policy-engine-wasm --test hl_exchange_deny_e2e`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use serde_json::{json, Value};

use policy_engine_wasm::evaluate_action_v2_json;

/// The off-chain-sig meta the converter emits.
fn hl_meta() -> Value {
    json!({
        "submitted_at": 1_738_000_000u64,
        "submitter": "0x000000000000000000000000000000000000a01c",
        "nature": {
            "kind": "offchain_sig",
            "domain": { "name": "Hyperliquid", "version": "1" },
            "deadline": 1_738_000_600u64
        }
    })
}

/// A HyperliquidCore order action JSON — the exact `hl-order-to-action.ts` shape
/// (doubly-tagged `{ domain, action, ...fields }`). `is_buy=false` ⇒ short.
fn order_action(is_buy: bool, size: &str) -> Value {
    json!({
        "domain": "hyperliquid_core",
        "action": "hl_order",
        "asset_index": 0,
        "symbol": "BTC",
        "is_buy": is_buy,
        "price": "60000",
        "size": size,
        "reduce_only": false,
        "tif": "gtc"
    })
}

/// A HyperliquidCore updateLeverage action JSON — `hl-order-to-action.ts` shape.
fn update_leverage_action(leverage: i64) -> Value {
    json!({
        "domain": "hyperliquid_core",
        "action": "hl_update_leverage",
        "asset_index": 0,
        "symbol": "BTC",
        "is_cross": true,
        "leverage": leverage
    })
}

/// A manifest triggering on a given HL Core action tag (no policy-RPC; the HL
/// deny/confirm rules read only base context).
fn manifest(tag: &str) -> Value {
    json!({
        "id": format!("{tag}-guard"),
        "schema_version": 2,
        "trigger": { "where": { "action.tag": { "eq": tag } } },
        "policy_rpc": [],
        "custom_context": { "fields": {} }
    })
}

const DENY_SHORT: &str = "\
@id(\"hl/no-short\")\n\
@severity(\"deny\")\n\
@reason(\"Short orders on Hyperliquid are blocked by policy\")\n\
forbid(principal, action == HyperliquidCore::Action::\"HlOrder\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context.side == \"short\" };\n";

/// Assemble the `EvaluateActionInput` envelope and run it through the entry
/// point. Returns the parsed output envelope.
fn run(action: Value, bundles: Value) -> Value {
    let input = json!({
        "action": action,
        "meta": hl_meta(),
        "tx": {
            "chain_id": "hl-mainnet",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x0000000000000000000000000000000000000000"
        },
        "bundles": bundles,
        "results": {}
    });
    serde_json::from_str(&evaluate_action_v2_json(input.to_string()))
        .expect("entry point returns JSON")
}

/// Read a shipped seed bundle (`policy.cedar` + `manifest.json`) verbatim — the
/// SAME artifact `copy-default-policies.js` ships into the extension.
fn seed_bundle(id: &str) -> Value {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("policy-engine")
        .join("tests")
        .join("fixtures")
        .join("default_policies_v2")
        .join(id);
    let policy = std::fs::read_to_string(dir.join("policy.cedar")).expect("read seed policy.cedar");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
            .expect("seed manifest.json parses");
    json!({ "policy": policy, "manifest": manifest })
}

/// THE PROOF (entry point): a Hyperliquid SHORT order returns a `fail` verdict.
#[test]
fn hyperliquid_short_order_denied_through_entry_point() {
    let parsed = run(
        order_action(false, "0.1"),
        json!([{ "policy": DENY_SHORT, "manifest": manifest("hl_order") }]),
    );
    assert_eq!(parsed["ok"], true, "envelope ok: {parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "fail",
        "short order must be DENIED: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl/no-short",
        "the deny rule must be the matched policy: {parsed}"
    );
}

/// CONTROL: a LONG order passes the short-only deny. Also proves a fractional
/// `size` ("0.1") deserializes cleanly (Decimal, not U256) — NOT a parse-failure
/// `__system__`/`__engine` deny.
#[test]
fn hyperliquid_long_order_passes_through_entry_point() {
    let parsed = run(
        order_action(true, "0.1"),
        json!([{ "policy": DENY_SHORT, "manifest": manifest("hl_order") }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "a long order must PASS the short-only deny: {parsed}"
    );
}

/// CONTROL: no bundle ⇒ baseline pass (blocking requires an explicit policy).
#[test]
fn no_bundle_passes_baseline_through_entry_point() {
    let parsed = run(order_action(false, "0.1"), json!([]));
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "no bundle ⇒ baseline pass: {parsed}"
    );
}

/// SHIPPED-SEED PROOF: the actual default bundle that ships in the extension
/// (`hl-no-short-perp/{policy.cedar,manifest.json}`, copied into
/// `public/default-policies/policy-set-v2.json`) DENIES a Hyperliquid short
/// order through `evaluate_action_v2_json`. This pins the SHIPPED policy ↔ the
/// HyperliquidCore action UID wiring (regression guard for the stale-fixture bug).
#[test]
fn shipped_seed_policy_denies_hyperliquid_short() {
    let parsed = run(
        order_action(false, "0.1"),
        json!([seed_bundle("hl-no-short-perp")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "fail",
        "the SHIPPED seed policy must DENY a HL short: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-no-short-perp",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

/// SHIPPED-SEED CONTROL: the shipped short-deny does NOT block a LONG order —
/// proves the retargeted policy is conditional on side, not a blanket fail.
#[test]
fn shipped_seed_policy_allows_hyperliquid_long() {
    let parsed = run(
        order_action(true, "0.1"),
        json!([seed_bundle("hl-no-short-perp")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "the shipped short-deny must let a long order PASS: {parsed}"
    );
}

/// D4 SHIPPED-SEED PROOF: the shipped `hl-confirm-withdraw` bundle FLAGS a
/// `withdraw3` for confirmation (`warn`) through the entry point — the
/// fund-movement action class is guarded, not just orders.
#[test]
fn shipped_seed_policy_confirms_hyperliquid_withdraw() {
    let withdraw = json!({
        "domain": "hyperliquid_core",
        "action": "hl_withdraw",
        "destination": "0x000000000000000000000000000000000000dead",
        "amount": "1000.5"
    });
    let parsed = run(withdraw, json!([seed_bundle("hl-confirm-withdraw")]));
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "the shipped confirm-withdraw policy must WARN: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-confirm-withdraw",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

/// D4 SHIPPED-SEED PROOF: the shipped `hl-confirm-high-leverage` bundle FLAGS a
/// high-leverage `updateLeverage` (`warn`) — closes the D4 gap where leverage
/// changes shipped no policy.
#[test]
fn shipped_seed_policy_confirms_high_leverage() {
    let parsed = run(
        update_leverage_action(25),
        json!([seed_bundle("hl-confirm-high-leverage")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "25x leverage must WARN: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-confirm-high-leverage",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

/// CONTROL: a modest leverage change (≤ 20x) is NOT flagged by the high-leverage
/// confirm — proves the guard is threshold-conditional, not a blanket warn.
#[test]
fn shipped_seed_policy_allows_modest_leverage() {
    let parsed = run(
        update_leverage_action(10),
        json!([seed_bundle("hl-confirm-high-leverage")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "10x leverage must PASS the >20x confirm: {parsed}"
    );
}
