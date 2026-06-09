//! End-to-end verdict golden for preset P4 (`peg-and-conversion-safety`) over the
//! NEW server-side enrichment method `oracle.steth_peg_status_bps`.
//!
//! P4's single SET (`stake-during-steth-discount-warn`) is dynamic: it warns when
//! a Lido stake happens while stETH trades at a >1% discount to ETH, reading the
//! `context.custom.stethDiscountBps` enrichment. The method that feeds it was
//! unimplemented (P4 dormant); it is now served by `policy-server` handler
//! (`oracle_steth_peg_status_bps`, ratio `price(stETH)/price(WETH)`).
//!
//! The WASM `evaluate_action_v2` replays an ALREADY-FETCHED `results` map into
//! `context.custom.*` (the SW dispatches the call to the policy-server). So these
//! tests inject the result directly — proving the manifest projection
//! (`$.result.discountBps → stethDiscountBps`) + the Cedar threshold
//! (`> decimal("100.0000")`) fire correctly, and that the policy is dormant
//! (fail-open) when the enrichment is absent. The method's own arithmetic is unit-
//! tested in `policy-server` (`handler::tests::steth_peg_*`).
//!
//! The preset is gitignored, so it is read at runtime and the test SKIPS when
//! absent. Install + route + evaluate on the same thread (WASM state is
//! thread-local).

use policy_engine_integration_tests::harness::{self, adapters};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const STETH: &str = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
/// call_id = `<manifest_id>::<rpc_id>` (the policy_rpc[0].id is "peg").
const PEG_CALL_ID: &str = "stake-during-steth-discount-warn::peg";

fn p4_set() -> Option<(String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../presets/liquid-staking-lido/p4-peg-and-conversion-safety/stake-during-steth-discount-warn",
    );
    let policy = fs::read_to_string(dir.join("policy.cedar")).ok()?;
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("manifest.json")).ok()?)
            .expect("P4 manifest parses");
    Some((policy, manifest))
}

/// Decode a real Lido `submit(address _referral)` (selector 0xa1903eab, value =
/// staked ETH) into a `LiquidStaking::Stake` (venue lido).
fn submit_env() -> Value {
    let calldata = format!("0xa1903eab{}", "0".repeat(64));
    harness::route::route_calldata(1, STETH, "0xa1903eab", &calldata, "1000000000000000000")
}

/// Evaluate the decoded stake against the P4 bundle with an injected results map.
fn eval(env: &Value, policy: &str, manifest: &Value, results: Value) -> Value {
    let action = env
        .pointer("/data/actions/0/body")
        .expect("route env carries data.actions[0].body");
    let meta = env
        .pointer("/data/actions/0/meta")
        .expect("route env carries data.actions[0].meta");
    let input = serde_json::json!({
        "action": action,
        "meta": meta,
        "tx": { "chain_id": "eip155:1", "from": "0x000000000000000000000000000000000000aaaa", "to": STETH },
        "bundles": [{ "policy": policy, "manifest": manifest }],
        "results": results,
    });
    let venv = harness::route::evaluate_action(&input);
    assert_eq!(
        venv.get("ok").and_then(Value::as_bool),
        Some(true),
        "evaluate_action_v2_json did not return ok: {venv}"
    );
    venv["data"]["verdict"].clone()
}

fn matched_has(verdict: &Value, policy_id: &str) -> bool {
    verdict["matched"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .any(|m| m.get("policy_id").and_then(Value::as_str) == Some(policy_id))
        })
        .unwrap_or(false)
}

#[test]
fn p4_peg_warns_when_steth_discount_exceeds_threshold() {
    let Some((policy, manifest)) = p4_set() else {
        eprintln!("P4 preset absent — skipping (gitignored)");
        return;
    };
    let _s = adapters::load_and_install().expect("install local surface");
    let env = submit_env();
    // 150 bps discount > 100 bps threshold ⇒ warn.
    let results = serde_json::json!({ PEG_CALL_ID: { "discountBps": "150.0000" } });
    let v = eval(&env, &policy, &manifest, results);
    assert_eq!(
        v.get("kind").and_then(Value::as_str),
        Some("warn"),
        "a Lido stake during a 150bps stETH discount must WARN: {v}"
    );
    assert!(
        matched_has(&v, "stake-during-steth-discount-warn"),
        "warn must be attributed to stake-during-steth-discount-warn: {v}"
    );
}

#[test]
fn p4_peg_dormant_without_enrichment() {
    let Some((policy, manifest)) = p4_set() else {
        eprintln!("P4 preset absent — skipping (gitignored)");
        return;
    };
    let _s = adapters::load_and_install().expect("install local surface");
    let env = submit_env();
    // No enrichment result ⇒ optional+has-guarded ⇒ dormant (fail-open) ⇒ pass.
    let v = eval(&env, &policy, &manifest, serde_json::json!({}));
    assert_eq!(
        v.get("kind").and_then(Value::as_str),
        Some("pass"),
        "absent peg enrichment must leave P4 dormant (pass), never SystemFail: {v}"
    );
}

#[test]
fn p4_peg_below_threshold_passes() {
    let Some((policy, manifest)) = p4_set() else {
        eprintln!("P4 preset absent — skipping (gitignored)");
        return;
    };
    let _s = adapters::load_and_install().expect("install local surface");
    let env = submit_env();
    // 50 bps discount < 100 bps threshold ⇒ no warn (peg within tolerance).
    let results = serde_json::json!({ PEG_CALL_ID: { "discountBps": "50.0000" } });
    let v = eval(&env, &policy, &manifest, results);
    assert_eq!(
        v.get("kind").and_then(Value::as_str),
        Some("pass"),
        "a 50bps discount (below the 100bps threshold) must PASS: {v}"
    );
}
