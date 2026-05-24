//! Curve NG-factory pool integration verification harness (R1.1 / F7).
//!
//! Backward-audit round R1.1 added 22 verified Curve factory pools across 3
//! pool-types — stableswap-ng-factory (13), twocrypto (5), factory-crypto (4)
//! — emitting 131 manifests (132 - 1 skip; LCAP/eUSD on Base lacks selector
//! `0xb2f9173e`). This harness asserts the post-generation registry is
//! installable, the callkey index routes correctly to the new bundles, and
//! the coin-substitution remap produced semantically correct emit rules for
//! at least one pool of each type.
//!
//! Scope: **structural** (parse + callkey + emit-rule asset addresses match
//! on-chain `coins()`). Not a real-calldata verification — `curve_realtx_tests`
//! in `policy-engine-wasm` already covers the full WASM decode→map→envelope
//! pipeline on real on-chain transactions.
//!
//! Source of truth:
//!   - `registry/scripts/curve-pool-targets.json` — curated pool list with
//!     verified on-chain (chain, address, coins) tuples.
//!   - `registry/manifests/curve/{stableswap-ng-factory,twocrypto,factory-crypto}/_template/*.json`
//!     — pool-type emit-rule templates (placeholder addresses; not installed).
//!   - `registry/manifests/curve/{pool-type}/{pool-name}/*.json` — generated
//!     per-pool bundles (output of `gen-from-sourcify.ts`).

#![cfg(test)]

use mappers::declarative::types::AdapterFunctionBundle;
use std::path::{Path, PathBuf};

// ───────────────────────────────────────────────────────────────────────────
// Locate the registry root + read helpers
// ───────────────────────────────────────────────────────────────────────────

fn registry_root() -> PathBuf {
    // `crates/adapters/mappers/tests/` → up 4 = repo root → registry/.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../registry")
        .canonicalize()
        .expect("registry root resolves")
}

/// Returns the list of pool-type subdirectory names this harness covers.
fn ng_factory_pool_types() -> &'static [&'static str] {
    &["stableswap-ng-factory", "twocrypto", "factory-crypto"]
}

fn read_callkey(chain_id: u64, to: &str, selector: &str) -> Option<serde_json::Value> {
    let callkey = format!(
        "{}__{}__{}.json",
        chain_id,
        to.to_lowercase(),
        selector.to_lowercase()
    );
    let path = registry_root().join("index/by-callkey").join(&callkey);
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| serde_json::from_str(&s).expect("callkey json parses"))
}

// ───────────────────────────────────────────────────────────────────────────
// Test 1 — every newly-generated manifest parses as AdapterFunctionBundle.
//
// Confirms the gen-from-sourcify.ts output is structurally valid registry
// content. Expects exactly 131 bundles (78 stableswap-ng-factory + 30 twocrypto
// - 1 lcap-eusd skip + 24 factory-crypto = 131).
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn ng_factory_bundles_parse_count_131_and_templates_not_indexed() {
    // -- Part 1: walk + parse every generated manifest (skipping _template/) --
    let mut total = 0usize;
    let mut per_type: std::collections::BTreeMap<String, usize> = Default::default();
    for ptype in ng_factory_pool_types() {
        let dir = registry_root().join("manifests").join("curve").join(ptype);
        assert!(dir.is_dir(), "pool-type dir missing: {dir:?}");
        let mut type_count = 0usize;
        walk_bundle_files(&dir, &mut |path| {
            let raw =
                std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
            let bundle: AdapterFunctionBundle = serde_json::from_str(&raw).unwrap_or_else(|e| {
                panic!("parse failed {path:?}: {e}\n--- raw ---\n{raw}\n--- end ---")
            });
            // sanity — id starts with "curve/<ptype>/<poolname>/"
            let prefix = format!("curve/{ptype}/");
            assert!(
                bundle.id.starts_with(&prefix),
                "{path:?}: id `{}` does not start with `{prefix}`",
                bundle.id
            );
            type_count += 1;
            total += 1;
        });
        per_type.insert(ptype.to_string(), type_count);
    }
    let expected_per_type = [
        ("stableswap-ng-factory", 78), // 13 pool × 6
        ("twocrypto", 29),             // 5 pool × 6 - 1 (lcap-eusd 0xb2f9173e bytecode-missing)
        ("factory-crypto", 24),        // 4 pool × 6
    ];
    for (k, want) in expected_per_type {
        let got = per_type.get(k).copied().unwrap_or(0);
        assert_eq!(
            got, want,
            "pool-type `{k}`: got {got} manifests, want {want}"
        );
    }
    assert_eq!(
        total, 131,
        "total NG-factory manifests = {total}, expected 131"
    );

    // -- Part 2: assert build-index.ts skip rule for _template/ holds --
    // Every template has placeholder pool 0x1111…1111. Its callkey would be
    // 1__0x1111111111111111111111111111111111111111__<selector>.json. None
    // such callkeys must exist in the live index.
    let template_selectors = [
        // stableswap-ng-factory 6
        "0xddc1f59d",
        "0xafb43012",
        "0xa7256d09",
        "0x2969e04a",
        "0x4a6e32c6",
        "0x081579a5",
        // twocrypto 6
        "0xa64833a0",
        "0x767691e7",
        "0x0c3e4b54",
        "0x3eb1719f",
        "0x0fbcee6e",
        "0xb2f9173e",
        // factory-crypto 6
        "0x5b41b908",
        "0x394747c5",
        "0xce7d6503",
        "0x0b4c7e4d",
        "0x5b36389c",
        "0xf1dc3cc9",
    ];
    let placeholder_pool = "0x1111111111111111111111111111111111111111";
    for sel in template_selectors {
        let entry = read_callkey(1, placeholder_pool, sel);
        assert!(
            entry.is_none(),
            "[template skip-rule fail] callkey 1__{placeholder_pool}__{sel}.json exists — \
             build-index.ts is indexing `_template/` content (placeholder pool 0x1111…1111 must \
             never appear in the live index)"
        );
    }
}

/// Recursively visits all `*.json` files under `root`, skipping `_template/`
/// directories (templates hold placeholder addresses, never installed).
fn walk_bundle_files(root: &Path, visit: &mut dyn FnMut(&Path)) {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read_dir {root:?}: {e}"))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("_template") {
                continue;
            }
            walk_bundle_files(&path, visit);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            visit(&path);
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Test 3 — callkey routing — 3 sample pools, one per pool-type.
//
// Picks one real pool from each new pool-type and confirms:
//   - the (chain_id, to, selector) callkey file exists,
//   - bundle_id matches the expected per-pool path.
// Anchors the generator + build-index pipeline end-to-end.
// ───────────────────────────────────────────────────────────────────────────

struct CallkeyCheck {
    chain_id: u64,
    to: &'static str,
    selector: &'static str,
    expected_bundle_id: &'static str,
}

const CALLKEY_CHECKS: &[CallkeyCheck] = &[
    // stableswap-ng-factory — RLUSD/USDC (Ethereum) exchange(int128,int128,uint256,uint256,address)
    CallkeyCheck {
        chain_id: 1,
        to: "0xd001ae433f254283fece51d4acce8c53263aa186",
        selector: "0xddc1f59d",
        expected_bundle_id: "curve/stableswap-ng-factory/rlusd-usdc/exchange-ng-receiver@1.0.0",
    },
    // twocrypto — YB cbBTC (Ethereum) exchange(uint256,uint256,uint256,uint256,address)
    CallkeyCheck {
        chain_id: 1,
        to: "0x83f24023d15d835a213df24fd309c47dab5beb32",
        selector: "0xa64833a0",
        expected_bundle_id: "curve/twocrypto/yb-cbbtc/exchange-twocrypto-receiver@1.0.0",
    },
    // factory-crypto — cbETH/WETH (Base) exchange(uint256,uint256,uint256,uint256,bool,address)
    CallkeyCheck {
        chain_id: 8453,
        to: "0x11c1fbd4b3de66bc0565779b35171a6cf3e71f59",
        selector: "0xce7d6503",
        expected_bundle_id: "curve/factory-crypto/cbeth-weth/exchange-fc-receiver@1.0.0",
    },
];

#[test]
fn callkey_index_routes_to_stableswap_ng_factory_pool() {
    let c = &CALLKEY_CHECKS[0];
    let entry = read_callkey(c.chain_id, c.to, c.selector)
        .unwrap_or_else(|| panic!("callkey {}__{}__{} missing", c.chain_id, c.to, c.selector));
    assert_eq!(
        entry["bundle_id"].as_str(),
        Some(c.expected_bundle_id),
        "routes to wrong bundle: {entry}"
    );
    assert_eq!(entry["bundle"]["match"]["chain_ids"][0], c.chain_id);
    assert_eq!(entry["bundle"]["match"]["to"][0].as_str(), Some(c.to));
}

#[test]
fn callkey_index_routes_to_twocrypto_pool() {
    let c = &CALLKEY_CHECKS[1];
    let entry = read_callkey(c.chain_id, c.to, c.selector)
        .unwrap_or_else(|| panic!("callkey {}__{}__{} missing", c.chain_id, c.to, c.selector));
    assert_eq!(
        entry["bundle_id"].as_str(),
        Some(c.expected_bundle_id),
        "routes to wrong bundle: {entry}"
    );
}

#[test]
fn callkey_index_routes_to_factory_crypto_pool() {
    let c = &CALLKEY_CHECKS[2];
    let entry = read_callkey(c.chain_id, c.to, c.selector)
        .unwrap_or_else(|| panic!("callkey {}__{}__{} missing", c.chain_id, c.to, c.selector));
    assert_eq!(
        entry["bundle_id"].as_str(),
        Some(c.expected_bundle_id),
        "routes to wrong bundle: {entry}"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Tests 4–6 — coin substitution sanity.
//
// For each pool-type representative, parse one swap bundle and verify the
// emit rule's `inputToken.asset.address` / `outputToken.asset.address` lookup
// arrays contain the on-chain coins (per `curve-pool-targets.json`, which was
// populated from Curve API + RPC `coins()`). Catches an address-remap bug
// that swapped coins[0] ↔ coins[1] or carried placeholder addresses through.
//
// Approach: read the bundle JSON, walk into `emit.fields["inputToken.asset.address"].args[0].literal[]`,
// compare against the expected coin set from the targets JSON.
// ───────────────────────────────────────────────────────────────────────────

fn read_bundle_raw(rel: &str) -> serde_json::Value {
    let path = registry_root().join(rel);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
}

/// Extract the coin-array literal from a swap bundle's emit rule:
/// `emit.fields["inputToken.asset.address"].args[0].literal`.
fn coin_array_from_swap_bundle(bundle: &serde_json::Value) -> Vec<String> {
    let fields = &bundle["emit"]["fields"];
    let lit = &fields["inputToken.asset.address"]["args"][0]["literal"];
    let arr = lit.as_array().expect("inputToken coin literal is array");
    arr.iter()
        .map(|v| {
            v.as_str()
                .expect("coin literal entry is string")
                .to_lowercase()
        })
        .collect()
}

#[test]
fn coin_substitution_stableswap_ng_factory_rlusd_usdc() {
    // RLUSD/USDC pool — chain 1 — Curve API getPools returned coinsAddresses:
    //   [0]=USDC 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48,
    //   [1]=RLUSD 0x8292bb45bf1ee4d140127049757c2e0ff06317ed.
    // The bundle's coin-array literal must preserve this order so the
    // select_from_literal_array fn with `from: $.args.i` resolves correctly.
    let bundle = read_bundle_raw(
        "manifests/curve/stableswap-ng-factory/rlusd-usdc/exchange-ng-receiver@1.0.0.json",
    );
    let coins = coin_array_from_swap_bundle(&bundle);
    let expected = [
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC (coins(0))
        "0x8292bb45bf1ee4d140127049757c2e0ff06317ed", // RLUSD (coins(1))
    ];
    assert_eq!(
        coins.len(),
        expected.len(),
        "expected {} coins, got {}",
        expected.len(),
        coins.len()
    );
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(
            coins[i], *want,
            "coin[{i}] — got {} want {}",
            coins[i], want
        );
    }
    // Pool address sanity — match.to must be the real pool, no placeholder leftover.
    assert_eq!(
        bundle["match"]["to"][0].as_str(),
        Some("0xd001ae433f254283fece51d4acce8c53263aa186")
    );
    // Selector must round-trip from the template.
    assert_eq!(bundle["match"]["selector"].as_str(), Some("0xddc1f59d"));
}

#[test]
fn coin_substitution_twocrypto_yb_wbtc() {
    // YB WBTC twocrypto-NG pool — chain 1 — coins:
    //   [0]=crvUSD 0xf939e0a03fb07f59a73314e73794be0e57ac1b4e,
    //   [1]=WBTC   0x2260fac5e5542a773aa44fbcfedf7c193bc2c599.
    let bundle =
        read_bundle_raw("manifests/curve/twocrypto/yb-wbtc/exchange-twocrypto-receiver@1.0.0.json");
    let coins = coin_array_from_swap_bundle(&bundle);
    let expected = [
        "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e",
        "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
    ];
    assert_eq!(coins.len(), expected.len());
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(
            coins[i], *want,
            "coin[{i}] — got {} want {}",
            coins[i], want
        );
    }
    assert_eq!(
        bundle["match"]["to"][0].as_str(),
        Some("0xd9ff8396554a0d18b2cfbec53e1979b7ecce8373")
    );
    // The new remove_liquidity_fixed_out manifest must exist for YB WBTC
    // (it has the selector in bytecode, unlike LCAP/eUSD on Base).
    let fixed_out = registry_root()
        .join("manifests/curve/twocrypto/yb-wbtc/removeLiquidityFixedOut-twocrypto@1.0.0.json");
    assert!(
        fixed_out.is_file(),
        "yb-wbtc removeLiquidityFixedOut manifest missing: {fixed_out:?}"
    );
    // Conversely, the LCAP/eUSD skip must be honored — that manifest must NOT exist.
    let lcap_fixed_out = registry_root()
        .join("manifests/curve/twocrypto/lcap-eusd/removeLiquidityFixedOut-twocrypto@1.0.0.json");
    assert!(
        !lcap_fixed_out.exists(),
        "lcap-eusd removeLiquidityFixedOut should be SKIPPED (selector missing in pool bytecode), \
         but file exists: {lcap_fixed_out:?}"
    );
}

#[test]
fn coin_substitution_factory_crypto_cbeth_weth() {
    // cbETH/WETH factory-crypto pool on Base (chain 8453) — coins:
    //   [0]=WETH  0x4200000000000000000000000000000000000006,
    //   [1]=cbETH 0x2ae3f1ec7f1f5012cfeab0185bfc7aa3cf0dec22.
    let bundle = read_bundle_raw(
        "manifests/curve/factory-crypto/cbeth-weth/exchange-fc-receiver@1.0.0.json",
    );
    let coins = coin_array_from_swap_bundle(&bundle);
    let expected = [
        "0x4200000000000000000000000000000000000006",
        "0x2ae3f1ec7f1f5012cfeab0185bfc7aa3cf0dec22",
    ];
    assert_eq!(coins.len(), expected.len());
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(
            coins[i], *want,
            "coin[{i}] — got {} want {}",
            coins[i], want
        );
    }
    assert_eq!(bundle["match"]["chain_ids"][0], 8453);
    assert_eq!(
        bundle["match"]["to"][0].as_str(),
        Some("0x11c1fbd4b3de66bc0565779b35171a6cf3e71f59")
    );
}
