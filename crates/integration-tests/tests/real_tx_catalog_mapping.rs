//! Real-tx → ActionBody → catalog-policy verdict, **up to the point right before
//! the `/v1/rpc` dispatch**.
//!
//! This closes the gap the `policy_catalog_v2` unit test leaves open: that test
//! proves each catalog policy *compiles* against its synthesised schema, but NOT
//! that it actually *fires* on a production-decoded transaction (trigger tag +
//! context fields really map from real calldata). Here we:
//!
//!   1. install the local `registryV2/index` bundles (the production decoder),
//!   2. feed **real on-chain calldata** (the committed Uniswap real-tx corpus +
//!      a plain unlimited-approve input) through `declarative_route_request_v3_json`
//!      → an `ActionBody` (the "mapping"),
//!   3. evaluate the **full 55-policy catalog** over that body with `results = {}`
//!      → a Cedar verdict. Empty results means enrichment calls are **dormant**
//!      (their `context.custom has <field>` guard is false), so this is exactly
//!      the state right before the host would POST to `/v1/rpc`. Pure-static
//!      policies produce their real verdict.
//!
//! Multicall (UR `execute`) bodies are fanned out per child, mirroring the SW
//! `evaluateBodyTree` recursion (Phase A) — so per-child policies are exercised
//! on real router calldata too.

use std::fs;
use std::path::{Path, PathBuf};

use policy_engine_integration_tests::harness::{self, adapters};
use serde_json::{json, Value};

fn catalog_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../policy-engine/tests/fixtures/policy_catalog_v2")
}

/// Recursively collect every leaf set dir (one containing `manifest.json`) under the
/// precedence-bucket tree (`compliance/ > protocol/ > wallet/ > action/`). `_`-prefixed
/// entries (`_methods/` impl-spec library) are skipped. Mirrors the walker in
/// `policy-engine/tests/policy_catalog_v2.rs` (two crates → small helper duplicated).
fn walk_catalog_sets(root: &Path) -> Vec<PathBuf> {
    let mut sets = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
        {
            continue;
        }
        if dir.join("manifest.json").is_file() {
            sets.push(dir);
            continue;
        }
        for entry in fs::read_dir(&dir).expect("read catalog dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    sets.sort();
    sets
}

/// Load every catalog set as an `evaluate_action_v2` bundle `{ policy, manifest }`.
fn load_catalog_bundles() -> Vec<Value> {
    let mut bundles = Vec::new();
    for dir in walk_catalog_sets(&catalog_dir()) {
        let manifest_str =
            fs::read_to_string(dir.join("manifest.json")).expect("read manifest.json");
        let policy = fs::read_to_string(dir.join("policy.cedar")).expect("read policy.cedar");
        let manifest: Value = serde_json::from_str(&manifest_str).expect("parse manifest");
        bundles.push(json!({ "policy": policy, "manifest": manifest }));
    }
    bundles
}

/// Evaluate one decoded body + recurse into Multicall children (mirrors the SW
/// `evaluateBodyTree`). Collects every matched `@id` and the worst verdict kind.
fn eval_body_tree(
    body: &Value,
    meta: &Value,
    tx: &Value,
    bundles: &[Value],
    matched: &mut Vec<String>,
    worst: &mut u8,
) {
    let domain = body.get("domain").and_then(Value::as_str).unwrap_or("");
    if domain != "unknown" {
        let eval_input = json!({
            "action": body, "meta": meta, "tx": tx,
            "bundles": bundles, "results": {},
        });
        let env = harness::route::evaluate_action(&eval_input);
        assert_eq!(
            env.get("ok").and_then(Value::as_bool),
            Some(true),
            "evaluate_action_v2_json returned a non-ok envelope: {env}"
        );
        let verdict = &env["data"]["verdict"];
        let kind = verdict
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("pass");
        *worst = (*worst).max(match kind {
            "fail" => 2,
            "warn" => 1,
            _ => 0,
        });
        if let Some(ids) = verdict.get("matched").and_then(Value::as_array) {
            for m in ids {
                if let Some(id) = m
                    .get("policy_id")
                    .and_then(Value::as_str)
                    .or_else(|| m.get("id").and_then(Value::as_str))
                    .or_else(|| m.as_str())
                {
                    matched.push(id.to_owned());
                }
            }
        }
    }
    if domain == "multicall" {
        if let Some(children) = body.get("actions").and_then(Value::as_array) {
            for child in children {
                eval_body_tree(child, meta, tx, bundles, matched, worst);
            }
        }
    }
}

/// Decode calldata via the production decoder, then evaluate the catalog tree.
/// Returns `(decoded_ok, top_domain, top_tag, matched_ids, worst_kind)`.
fn map_and_evaluate(
    chain_id: u64,
    to: &str,
    calldata: &str,
    value: &str,
    bundles: &[Value],
) -> (bool, String, String, Vec<String>, &'static str) {
    let selector = &calldata[..10.min(calldata.len())];
    // route_calldata wants `value` as a DECIMAL wei string. Normalize hex
    // (`0x..`, as eth_getTransactionByHash returns) → decimal; pass decimal as-is.
    let value_dec = match value.strip_prefix("0x") {
        Some(hex) => u128::from_str_radix(hex, 16)
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "0".to_owned()),
        None => value.to_owned(),
    };
    let env = harness::route::route_calldata(chain_id, to, selector, calldata, &value_dec);
    if env.get("ok").and_then(Value::as_bool) != Some(true) {
        return (false, String::new(), String::new(), Vec::new(), "pass");
    }
    let actions = match env["data"]["actions"].as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return (false, String::new(), String::new(), Vec::new(), "pass"),
    };
    let tx = json!({
        "chain_id": format!("eip155:{chain_id}"),
        "from": "0x000000000000000000000000000000000000aaaa",
        "to": to,
    });
    let mut matched = Vec::new();
    let mut worst = 0u8;
    let top = &actions[0];
    let body = &top["body"];
    let meta = &top["meta"];
    let domain = body
        .get("domain")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let tag = body
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    for a in actions {
        eval_body_tree(
            &a["body"],
            &a["meta"],
            &tx,
            bundles,
            &mut matched,
            &mut worst,
        );
    }
    matched.sort();
    matched.dedup();
    let kind = match worst {
        2 => "fail",
        1 => "warn",
        _ => "pass",
    };
    let _ = (body, meta);
    (true, domain, tag, matched, kind)
}

/// An unlimited ERC-20 `approve(spender, type(uint256).max)` to a registry-COVERED
/// token (USDC) must decode to `Token::Erc20Approve` with `amount = U256::MAX` and
/// trip `unlimited-erc20-approve` (DENY). Exercises the production decoder + a real
/// per-token callkey + the catalog's deny path at the verdict level (the unit test
/// only proves the policy compiles; this proves it actually denies).
#[test]
fn unlimited_approve_to_covered_token_denies() {
    let _ = adapters::load_and_install().expect("install registryV2 index");
    let bundles = load_catalog_bundles();

    // USDC (mainnet) — covered by the local index for selector 0x095ea7b3.
    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    // approve(0x1111…1111, type(uint256).max): selector + 32B spender + 32B MAX.
    let calldata = concat!(
        "0x095ea7b3",
        "0000000000000000000000001111111111111111111111111111111111111111",
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    );

    let (ok, domain, tag, matched, kind) = map_and_evaluate(1, USDC, calldata, "0", &bundles);
    eprintln!(
        "[unlimited-approve→USDC] decoded_ok={ok} domain={domain} tag={tag} verdict={kind} matched={matched:?}"
    );
    assert!(
        ok,
        "unlimited approve to a covered token (USDC) must decode"
    );
    assert_eq!(domain, "token", "must map to the token domain");
    assert_eq!(tag, "erc20_approve", "must map to erc20_approve");
    assert_eq!(
        kind, "fail",
        "an unlimited approve must DENY; matched={matched:?}"
    );
    assert!(
        matched.iter().any(|m| m == "unlimited-erc20-approve"),
        "unlimited-erc20-approve must be the matched policy; got {matched:?}"
    );
}

/// Live end-to-end: a transaction fetched from the **Etherscan API** → production
/// decode → catalog verdict (right before `/v1/rpc`). Gated on env so a no-network
/// run skips it; the companion shell step fetches a real tx and sets these:
///   `SCOPEBALL_LIVE_TO`, `SCOPEBALL_LIVE_CALLDATA` (req), `SCOPEBALL_LIVE_CHAIN`,
///   `SCOPEBALL_LIVE_VALUE` (opt).
#[test]
fn live_etherscan_tx_maps() {
    let (to, calldata) = match (
        std::env::var("SCOPEBALL_LIVE_TO"),
        std::env::var("SCOPEBALL_LIVE_CALLDATA"),
    ) {
        (Ok(to), Ok(cd)) if cd.len() >= 10 => (to, cd),
        _ => {
            eprintln!("[live] skipped — set SCOPEBALL_LIVE_TO + SCOPEBALL_LIVE_CALLDATA (from an Etherscan fetch) to run");
            return;
        }
    };
    let chain: u64 = std::env::var("SCOPEBALL_LIVE_CHAIN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let value = std::env::var("SCOPEBALL_LIVE_VALUE").unwrap_or_else(|_| "0x0".to_owned());

    let _ = adapters::load_and_install().expect("install registryV2 index");
    let bundles = load_catalog_bundles();
    let (ok, domain, tag, matched, kind) =
        map_and_evaluate(chain, &to, &calldata, &value, &bundles);
    eprintln!(
        "[live] Etherscan tx to={to} chain={chain} decoded_ok={ok} domain={domain} tag={tag} verdict={kind} matched={matched:?}"
    );
    assert!(
        ok,
        "live Etherscan-fetched tx must decode through the production decoder (to={to})"
    );
}

/// Every committed real Uniswap tx (`expect: "pass"`) decodes via the production
/// decoder, and the full catalog evaluates over it (top-level + per-child) without
/// fault. Asserts the catalog *engages* real data: ≥1 policy fires across the
/// corpus, and v2-router swaps map to `amm/swap`. Prints the per-tx verdict.
#[test]
fn corpus_real_txs_map_and_evaluate() {
    let _ = adapters::load_and_install().expect("install registryV2 index");
    let bundles = load_catalog_bundles();

    let corpus: Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/golden/uniswap-real-tx/corpus.json"),
        )
        .expect("read uniswap real-tx corpus"),
    )
    .expect("parse corpus");
    let txs = corpus["transactions"].as_array().expect("transactions[]");

    let mut decoded = 0usize;
    let mut fired = 0usize;
    let mut all_matched: Vec<String> = Vec::new();
    let mut saw_amm_swap = false;

    for t in txs {
        if t.get("expect").and_then(Value::as_str) != Some("pass") {
            continue;
        }
        let p = &t["rpc"]["params"][0];
        let (to, data) = match (p["to"].as_str(), p["data"].as_str()) {
            (Some(to), Some(data)) if data.len() >= 10 => (to, data),
            _ => continue,
        };
        let value = p.get("value").and_then(Value::as_str).unwrap_or("0x0");
        let chain_id = t["chain_id"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1u64);

        let (ok, domain, tag, matched, kind) =
            map_and_evaluate(chain_id, to, data, value, &bundles);
        let intent = t.get("intent").and_then(Value::as_str).unwrap_or("?");
        eprintln!(
            "[corpus] intent={intent} to={to} decoded_ok={ok} domain={domain} tag={tag} verdict={kind} matched={matched:?}"
        );
        if !ok {
            continue;
        }
        decoded += 1;
        if domain == "amm" && tag == "swap" {
            saw_amm_swap = true;
        }
        if !matched.is_empty() {
            fired += 1;
            all_matched.extend(matched);
        }
    }

    all_matched.sort();
    all_matched.dedup();
    eprintln!("[corpus] SUMMARY decoded={decoded} txs_with_a_fire={fired} distinct_policies_fired={all_matched:?}");

    assert!(
        decoded >= 5,
        "expected ≥5 real corpus txs to decode; got {decoded}"
    );
    assert!(
        saw_amm_swap,
        "expected ≥1 v2-router swap to map to amm/swap"
    );
    assert!(
        fired >= 1,
        "expected ≥1 catalog policy to fire across the real corpus (catalog must engage real data)"
    );
}
