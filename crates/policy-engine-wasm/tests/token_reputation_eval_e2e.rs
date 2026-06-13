//! E2E: the shipped `approve-spender-reputation-deny` token policy evaluated
//! through the literal extension entry point `evaluate_action_v2_json` — the SAME
//! engine path the service-worker runs: lower `Erc20Approve` → plan the
//! `address.reputation` RPC → replay its result into `context.custom.spenderFlagged`
//! → Cedar → aggregate verdict.
//!
//! This closes the loop the browser harness cannot (this build's ps2/block-IR
//! policy store drops `context.custom.*` policies at cedar→IR conversion, so a
//! custom enrichment policy can't be installed via the UI path). Here the policy
//! is evaluated by the real WASM engine directly, proving it APPLIES.
//!
//! The `spenderFlagged` bit is exactly what the server's `address.reputation`
//! method (GoPlus) returns — verified LIVE in the `/evaluate` smoke this session
//! (Lazarus `0x098b71…` → `flagged:true`, vitalik → `false`).
//!
//! Shipped `policy.cedar` / `manifest.json` are loaded from disk each run (drift
//! fails here).
//!
//! Run: `cargo test -p policy-engine-wasm --test token_reputation_eval_e2e`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use serde_json::{json, Value};

use policy_engine_wasm::evaluate_action_v2_json;

const POLICY: &str = "approve-spender-reputation-deny";
/// `manifest_id::spec_id` — spec id is the manifest's `policy_rpc[0].id`.
const CALL: &str = "approve-spender-reputation-deny::spender-rep";

const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const LAZARUS: &str = "0x098b716b8aaf21512996dc57eb0615e2383e2f96"; // GoPlus → flagged:true (live)
const CLEAN: &str = "0xd8da6bf26964af9d7eed9e03e53415d37aa96045"; // vitalik EOA → flagged:false
const SUBMITTER: &str = "0x1111111111111111111111111111111111111111";

fn preset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../presets/token/policy-set/approve-spender-reputation-deny")
}
/// The shipped policy lives in the gitignored preset lab tree; a fresh
/// checkout (CI) lacks it, so each test SKIPS when the preset dir is absent —
/// mirroring the preset compile gates. Run locally (presets/ present) for real
/// coverage. Returns `true` when the test body should be skipped.
fn preset_absent() -> bool {
    if preset_dir().is_dir() {
        return false;
    }
    eprintln!("approve-spender-reputation-deny preset absent — skipping (gitignored lab tree)");
    true
}
fn cedar() -> String {
    std::fs::read_to_string(preset_dir().join("policy.cedar"))
        .unwrap_or_else(|e| panic!("read policy.cedar: {e}"))
}
fn manifest() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(preset_dir().join("manifest.json"))
            .unwrap_or_else(|e| panic!("read manifest.json: {e}")),
    )
    .expect("manifest parses")
}
fn bundle() -> Value {
    json!({ "policy": cedar(), "manifest": manifest() })
}

/// `approve(spender, amount)` — finite amount so ONLY the reputation policy is in
/// play (no unlimited-approval interaction; this test installs only this bundle).
fn approve_action(spender: &str) -> Value {
    json!({
        "domain": "token",
        "action": "erc20_approve",
        "token": { "key": { "standard": "erc20", "chain": "eip155:1", "address": USDC } },
        "spender": spender,
        "amount": "0x5f5e100"
    })
}

fn meta() -> Value {
    json!({
        "submitted_at": 1_738_000_000u64,
        "submitter": SUBMITTER,
        "nature": {
            "kind": "onchain_tx",
            "chain": "eip155:1",
            "nonce": 0,
            "gas_limit": "0x0",
            "gas_price": {
                "value": "0x0",
                "source": { "kind": "oracle_feed", "provider": "pyth", "feed_id": "gas/eip155:1" },
                "synced_at": 1_738_000_000u64
            },
            "value": "0x0"
        }
    })
}

fn run(spender: &str, results: Value) -> Value {
    let input = json!({
        "action": approve_action(spender),
        "meta": meta(),
        "tx": { "chain_id": "eip155:1", "from": SUBMITTER, "to": USDC },
        "bundles": json!([bundle()]),
        "results": results
    });
    serde_json::from_str(&evaluate_action_v2_json(input.to_string())).expect("entry returns JSON")
}

fn kind(p: &Value) -> String {
    assert_eq!(p["ok"], true, "envelope not ok: {p}");
    p["data"]["verdict"]["kind"]
        .as_str()
        .unwrap_or("<none>")
        .to_owned()
}
fn matched_ids(p: &Value) -> Vec<String> {
    p["data"]["verdict"]["matched"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["policy_id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
/// The result shape the server's `address.reputation` projects (`$.result.flagged`).
fn reputation(flagged: bool) -> Value {
    json!({ CALL: { "flagged": flagged } })
}

// ═════════════════════════════════════════════════════════════════════════════
// approve-spender-reputation-deny  (DENY on a GoPlus-flagged spender)
// ═════════════════════════════════════════════════════════════════════════════

/// Flagged spender (the GoPlus `flagged:true` the live smoke returned for Lazarus)
/// ⇒ the forbid fires ⇒ DENY (`kind == "fail"`).
#[test]
fn flagged_spender_denies() {
    if preset_absent() {
        return;
    }
    let p = run(LAZARUS, reputation(true));
    assert_eq!(kind(&p), "fail", "flagged spender must DENY: {p}");
    assert!(
        matched_ids(&p).contains(&POLICY.to_string()),
        "reputation deny must be the matched policy: {p}"
    );
}

/// Clean spender (GoPlus `flagged:false`) ⇒ the forbid stays dormant ⇒ PASS.
#[test]
fn clean_spender_passes() {
    if preset_absent() {
        return;
    }
    let p = run(CLEAN, reputation(false));
    assert_eq!(kind(&p), "pass", "clean spender must PASS: {p}");
}

/// 🔓 deny-on-OPTIONAL is FAIL-OPEN: the manifest's `address.reputation` output is
/// `required:false` / `optional:true`, so an OMITTED result (GoPlus unavailable)
/// leaves `spenderFlagged` ABSENT ⇒ the forbid is dormant ⇒ PASS — NOT a deny.
/// (This is the characteristic flagged in the policy review: a deny gating on a
/// fallible third-party feed fail-opens when the feed is down. Documented here so
/// the behavior is pinned; flipping to `required:true` would make it fail-closed.)
#[test]
fn omitted_reputation_fail_opens_to_pass() {
    if preset_absent() {
        return;
    }
    let p = run(LAZARUS, json!({}));
    assert_eq!(
        kind(&p),
        "pass",
        "omitted reputation fail-OPENs (deny-on-optional): {p}"
    );
}
