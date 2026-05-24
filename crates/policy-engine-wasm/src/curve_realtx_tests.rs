//! Part 5 — Curve real-transaction coverage verification harness.
//!
//! Feeds 29 real on-chain Curve transactions (sampled via Dune Analytics over a
//! 120-day window, raw `ethereum.transactions.data` calldata) through
//! `declarative_route_request_json` — the production WASM decode entry — and
//! classifies each outcome. Diagnostic harness for `docs/CURVE-REALTX-VERIFICATION.md`.
//!
//! Bundles are installed ONLY from the local `registry/manifests/curve/**`
//! tree (no network — `cargo test` has no I/O to the GCP registry).
//!
//! Run:  cargo test -p policy-engine-wasm curve_realtx -- --nocapture
//!
//! Outcome taxonomy:
//!   covered sample -> expect ok:true            (a fail = ❌ fault finding)
//!   gap sample     -> expect ok:false, kind=no_declarative_mapper
//!                     (ok:true = a wrong-bundle misdecode finding)
//! Envelope *semantic* correctness (⚠️ mis-decode) is judged in the report from
//! the printed envelope JSON vs Etherscan ground-truth — not asserted broadly,
//! since hardcoding 20 expected envelopes would re-introduce desk-check error.
//! EXCEPTION — `check_swap` asserts input/output token addresses as a regression
//! guard for a shipped fix (Part 6 F2 — crvUSD-NG coin-array inversion). Its
//! expected values are NOT hand-computed: they come from on-chain `coins()`
//! (RPC) + `cast`-decoded calldata. The broad rule still holds for rt01-rt29.

#![cfg(test)]

use crate::{declarative_install_json, declarative_route_request_json};
use serde_json::{json, Value};
use std::path::Path;

/// Install every local Curve manifest into this thread's declarative state.
/// Called per-test: `DECLARATIVE_STATE` is thread-local and each `#[test]`
/// runs on its own thread.
fn install_all_curve() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../registry/manifests/curve"
    );
    let mut count = 0usize;
    install_dir(Path::new(dir), &mut count);
    assert!(
        count > 100,
        "expected >100 curve manifests, installed {count}"
    );
}

fn install_dir(p: &Path, count: &mut usize) {
    let mut entries: Vec<_> = std::fs::read_dir(p)
        .unwrap_or_else(|e| panic!("read_dir {p:?}: {e}"))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            // `_template/` dir 은 R1.1 의 generator 가 사용하는 placeholder
            // template 보관용 — install 대상 아님 (__POOL__/__CHAIN_ID__ 가
            // bundle schema 에서 invalid). build-index.ts + audit-addresses.ts
            // 도 동일 skip rule.
            if path.file_name().and_then(|n| n.to_str()) == Some("_template") {
                continue;
            }
            install_dir(&path, count);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let bundle = std::fs::read_to_string(&path).unwrap();
            let out = declarative_install_json(bundle);
            let v: Value = serde_json::from_str(&out).unwrap();
            assert_eq!(v["ok"], true, "install failed {path:?}: {out}");
            *count += 1;
        }
    }
}

struct Sample {
    label: &'static str,
    chain_id: u64,
    to: &'static str,
    selector: &'static str,
    from: &'static str,
    value: &'static str,
    calldata: &'static str,
}

fn route(s: &Sample) -> Value {
    install_all_curve();
    let input = json!({
        "chain_id": s.chain_id,
        "to": s.to,
        "selector": s.selector,
        "ctx": {
            "chain_id": s.chain_id,
            "from": s.from,
            "to": s.to,
            "value_wei": s.value,
            "block_timestamp": 1_726_000_000_u64
        },
        "calldata": s.calldata,
    });
    let out = declarative_route_request_json(input.to_string());
    serde_json::from_str(&out).expect("route output is valid json")
}

/// Covered sample: production path must produce a declarative envelope.
fn check_covered(s: &Sample) {
    let v = route(s);
    println!(
        "\n=== [COVERED] {} ===\n  to={}  selector={}\n  result={}",
        s.label,
        s.to,
        s.selector,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(
        v["ok"], true,
        "[{}] expected a declarative HIT but routing failed: {}",
        s.label, v
    );
}

/// Gap sample: no bundle is bridged for this (chain, to, selector) — the
/// orchestrator must cleanly miss to the static fallback. `ok:true` here means
/// a wrong bundle matched (a mis-decode finding).
fn check_gap(s: &Sample) {
    let v = route(s);
    println!(
        "\n=== [GAP] {} ===\n  to={}  selector={}\n  result={}",
        s.label,
        s.to,
        s.selector,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(
        v["ok"], false,
        "[{}] expected a clean MISS but it routed to a bundle: {}",
        s.label, v
    );
    assert_eq!(
        v["error"]["kind"], "no_declarative_mapper",
        "[{}] expected error kind no_declarative_mapper: {}",
        s.label, v
    );
}

/// Covered swap sample with input/output token assertion — a regression guard
/// for a shipped fix (Part 6 F2). `expect_in`/`expect_out` are ground-truth
/// token addresses (lowercase) from on-chain `coins()` + `cast`-decoded
/// calldata — see module docstring for why this is not a desk-check.
fn check_swap(s: &Sample, expect_in: &str, expect_out: &str) {
    let v = route(s);
    println!(
        "\n=== [SWAP] {} ===\n  to={}  selector={}\n  result={}",
        s.label,
        s.to,
        s.selector,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(
        v["ok"], true,
        "[{}] expected a declarative HIT but routing failed: {}",
        s.label, v
    );
    let fields = &v["data"]["envelopes"][0]["fields"];
    assert_eq!(
        fields["inputToken"]["asset"]["address"].as_str(),
        Some(expect_in),
        "[{}] inputToken address — expected {}",
        s.label,
        expect_in
    );
    assert_eq!(
        fields["outputToken"]["asset"]["address"].as_str(),
        Some(expect_out),
        "[{}] outputToken address — expected {}",
        s.label,
        expect_out
    );
}

// ─────────── samples — real on-chain calldata, Dune 120-day window ───────────

const CD01: &str = "0x37671f93000000000000000000000000000000000000000000003f859393b6c15d70f34100000000000000000000000046a83dc1a264bff133db887023d288416709483700000000000000000000000000000000000000000000000000000000000000180000000000000000000000000000000000000000000000000000000000000000";
#[test]
fn rt01_crvusd_repay() {
    check_covered(&Sample {
        label: "crvUSD wstETH Controller repay",
        chain_id: 1,
        to: "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce",
        selector: "0x37671f93",
        from: "0x46a83dc1a264bff133db887023d2884167094837",
        value: "0",
        calldata: CD01,
    });
}

const CD02: &str = "0xdd171e7c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001b1ae4d6e2ef500000";
#[test]
fn rt02_crvusd_borrow_more() {
    // Part 7 F6 — borrow_more emits a `collateralAmount` leg. CD02 args
    // (cast abi-decode "borrow_more(uint256,uint256)"): collateral=0, debt=5e20.
    // borrow_more without adding collateral → collateralAmount.value == "0".
    let s = Sample {
        label: "crvUSD wstETH Controller borrow_more (F6 fix)",
        chain_id: 1,
        to: "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce",
        selector: "0xdd171e7c",
        from: "0xdb4296e52e34ff945ee8bfb73a96c3e3f5aee321",
        value: "0",
        calldata: CD02,
    };
    let v = route(&s);
    println!(
        "\n=== [F6 borrow_more] {} ===\n  {}",
        s.label,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(v["ok"], true, "[rt02] expected a declarative HIT: {v}");
    let f = &v["data"]["envelopes"][0]["fields"];
    assert_eq!(
        f["collateralAmount"]["value"].as_str(),
        Some("0"),
        "[rt02] collateralAmount.value must be 0 (borrow_more added no collateral)"
    );
}

const CD03: &str = "0x23cfed030000000000000000000000000000000000000000000000003950c1d532eaf02500000000000000000000000000000000000000000000014542ba12a337c000000000000000000000000000000000000000000000000000000000000000000009";
#[test]
fn rt03_crvusd_create_loan() {
    // Part 7 F6 — create_loan emits a `collateralAmount` leg. CD03 args
    // (cast abi-decode "create_loan(uint256,uint256,uint256)"):
    // collateral=4130013979725197349, debt=6e21, N=9.
    let s = Sample {
        label: "crvUSD wstETH Controller create_loan (F6 fix)",
        chain_id: 1,
        to: "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce",
        selector: "0x23cfed03",
        from: "0x55f03b94065dcdacb0df05966643789cd4dc8b85",
        value: "0",
        calldata: CD03,
    };
    let v = route(&s);
    println!(
        "\n=== [F6 create_loan] {} ===\n  {}",
        s.label,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(v["ok"], true, "[rt03] expected a declarative HIT: {v}");
    let f = &v["data"]["envelopes"][0]["fields"];
    assert_eq!(
        f["collateralAmount"]["value"].as_str(),
        Some("4130013979725197349"),
        "[rt03] collateralAmount.value must be the create_loan collateral arg"
    );
}

const CD04: &str = "0x2e1a7d4d000000000000000000000000000000000000000000000000017d916070b4d8aa";
#[test]
fn rt04_gauge_steth_withdraw() {
    check_covered(&Sample {
        label: "stETH gauge withdraw",
        chain_id: 1,
        to: "0x182b723a58739a9c974cfdb385ceadb237453c28",
        selector: "0x2e1a7d4d",
        from: "0x92edd8e94b0cfcd4efe07b0f01f1e87d9ce2d2d4",
        value: "0",
        calldata: CD04,
    });
}

const CD05: &str = "0xb6b55f250000000000000000000000000000000000000000000000003cfc188e2e691700";
#[test]
fn rt05_gauge_steth_deposit() {
    check_covered(&Sample {
        label: "stETH gauge deposit",
        chain_id: 1,
        to: "0x182b723a58739a9c974cfdb385ceadb237453c28",
        selector: "0xb6b55f25",
        from: "0xc60b9019bbccf651855e2e208429a2ec5bcf2e63",
        value: "0",
        calldata: CD05,
    });
}

const CD06: &str = "0x1d2747d400000000000000000000000056c526b0159a258887e0d79ec3a80dfb940d0cd70000000000000000000000000000000000000000000000000000000000000001";
#[test]
fn rt06_gauge_steth_withdraw_overload() {
    check_gap(&Sample {
        label: "stETH gauge withdraw 2-arg overload 0x1d2747d4 - UNCOVERED",
        chain_id: 1,
        to: "0x182b723a58739a9c974cfdb385ceadb237453c28",
        selector: "0x1d2747d4",
        from: "0x43b9c722748c93ab89af750e32dbd7f68f2f3fa1",
        value: "0",
        calldata: CD06,
    });
}

const CD07: &str = "0xe6f1daf2";
#[test]
fn rt07_gauge_steth_claim_rewards() {
    check_covered(&Sample {
        label: "stETH gauge claim_rewards",
        chain_id: 1,
        to: "0x182b723a58739a9c974cfdb385ceadb237453c28",
        selector: "0xe6f1daf2",
        from: "0x393c662e5db954a8d34ea483812f15497fb52825",
        value: "0",
        calldata: CD07,
    });
}

const CD08: &str = "0xd713632800000000000000000000000026f7786de3e6d9bd37fcf47be6f2bc455a21b74a0000000000000000000000000000000000000000000000000000000000000cb0";
#[test]
fn rt08_gaugecontroller_vote() {
    // Part 7 F5 — vote_for_gauge_weights. Previously emitted a `vote`
    // (governance-proposal) envelope with a fabricated proposalId "0" and a
    // mislabelled `governance`. After F5: a `gauge_vote` envelope — gauge in
    // `pools`, weight in `weights`, no tokenId (Curve veCRV is account-bound).
    let s = Sample {
        label: "GaugeController vote_for_gauge_weights (F5 fix)",
        chain_id: 1,
        to: "0x2f50d538606fa9edd2b11e2446beb18c9d5846bb",
        selector: "0xd7136328",
        from: "0x4986d3b5160032ab7df0fac9503f6a2360f3f888",
        value: "0",
        calldata: CD08,
    };
    let v = route(&s);
    println!(
        "\n=== [F5 gauge_vote] {} ===\n  {}",
        s.label,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(v["ok"], true, "[rt08] expected covered after F5: {v}");
    let env = &v["data"]["envelopes"][0];
    assert_eq!(env["action"].as_str(), Some("gauge_vote"), "[rt08] action");
    let f = &env["fields"];
    assert_eq!(
        f["pools"][0].as_str(),
        Some("0x26f7786de3e6d9bd37fcf47be6f2bc455a21b74a"),
        "[rt08] pools[0] must be the gauge address"
    );
    assert_eq!(f["weights"][0].as_str(), Some("3248"), "[rt08] weights[0]");
}

const CD09: &str = "0x5c9c18e2000000000000000000000000085780639cc2cacd35e474e71f4d000e2405d8f60000000000000000000000005018be882dcce5e3f2f3b0913ae2096b9b3fb61f000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb480000000000000000000000007f86bf177dd4f3494b841a37e810a34dd56c829b000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000ee351f12eae8c2b8b9d1b9bfd3c5dd565234578d000000000000000000000000419905009e4656fdc02418c7df35b1e61ed5f72600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010db1fe8d52005c0000000000000000000000000000000000000000000000000aca9d47c4f69f7685910000000000000000000000005018be882dcce5e3f2f3b0913ae2096b9b3fb61f0000000000000000000000007f86bf177dd4f3494b841a37e810a34dd56c829b0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ee351f12eae8c2b8b9d1b9bfd3c5dd565234578d0000000000000000000000000000000000000000000000000000000000000000";
#[test]
fn rt09_router_ng_exchange_5arg() {
    // Part 6 F1 — 5-arg `exchange` (0x5c9c18e2) is the dominant real router
    // variant (54,724 tx / 120d). Previously a clean MISS; now covered by the
    // `exchange-5arg-*` bundles. The gap→covered flip is the F1 fix evidence.
    check_covered(&Sample {
        label: "router-ng exchange 5-arg - dominant real router variant (F1 fix)",
        chain_id: 1,
        to: "0x45312ea0eff7e09c83cbe249fa1d7598c4c8cd4e",
        selector: "0x5c9c18e2",
        from: "0xd13be92afe0041e5510fef13a21410fcaecd4081",
        value: "0",
        calldata: CD09,
    });
}

const CD10: &str = "0xc872a3c500000000000000000000000023238f20b894f29041f48d88ee91131c395aaa71000000000000000000000000f4d0cf32908b2c7f1021339c43df0f77f06896d7000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000079d38000000000000000000000000000000000000000000000000000000000006da52000000000000000000000000f4d0cf32908b2c7f1021339c43df0f77f06896d70000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e3474c5ccf3e882b442f2b46c43f1ae3c17f3887";
#[test]
fn rt10_router_ng_exchange_6arg() {
    check_covered(&Sample {
        label: "router-ng exchange 6-arg - covered router bundle selector",
        chain_id: 1,
        to: "0x45312ea0eff7e09c83cbe249fa1d7598c4c8cd4e",
        selector: "0xc872a3c5",
        from: "0xe3474c5ccf3e882b442f2b46c43f1ae3c17f3887",
        value: "0",
        calldata: CD10,
    });
}

const CD11: &str = "0x371dc447000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee0000000000000000000000007f86bf177dd4f3494b841a37e810a34dd56c829b000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000bebc44782c7db0a1a60cb6fe97d0b483032ff1c7000000000000000000000000dac17f958d2ee523a2206206994597c13d831ec7000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000038d7ea4c680000000000000000000000000000000000000000000000000000000000000233258";
#[test]
fn rt11_router_ng_exchange_4arg() {
    // Part 6 F1 — 4-arg `exchange` (0x371dc447) is real but negligible
    // (120-day window: 1 tx). Intentionally left uncovered — adding 5 bundles
    // for a single tx is not worth the registry bloat. Stays a clean MISS.
    check_gap(&Sample {
        label: "router-ng exchange 4-arg overload - 120d 1tx, intentionally uncovered",
        chain_id: 1,
        to: "0x45312ea0eff7e09c83cbe249fa1d7598c4c8cd4e",
        selector: "0x371dc447",
        from: "0x201def01385f1d2319f8de7c432decb5b3491eb2",
        value: "1000000000000000",
        calldata: CD11,
    });
}

const CD12: &str = "0x0b4c7e4d000000000000000000000000000000000000000000000000000000000000c35000000000000000000000000000000000000000000000000000b1a2bc2ec5000000000000000000000000000000000000000000000000000001597ef143ce7ae9";
#[test]
fn rt12_ng_crvusd_usdc_add_liquidity() {
    // Part 7 F3 — add_liquidity 0x0b4c7e4d. Previously UNCOVERED (bundle had the
    // receiver-variant selector 0x0c3e4b54). After F3: covered, and the
    // inputTokens asset literal order corrected to pool coins() = [USDC, crvUSD]
    // (was reversed — `_amounts[0]` is coins[0]=USDC amount, not crvUSD).
    let s = Sample {
        label: "crvUSD/USDC NG pool add_liquidity (F3 fix)",
        chain_id: 1,
        to: "0x4dece678ceceb27446b35c672dc7d61f30bad69e",
        selector: "0x0b4c7e4d",
        from: "0xf5b74389147b55c1edc9a15abb06bbabf7c78c33",
        value: "0",
        calldata: CD12,
    };
    let v = route(&s);
    println!(
        "\n=== [F3 add_liquidity] {} ===\n  {}",
        s.label,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(v["ok"], true, "[rt12] expected covered after F3: {v}");
    let f = &v["data"]["envelopes"][0]["fields"];
    assert_eq!(
        f["inputTokens"][0]["asset"]["address"].as_str(),
        Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        "[rt12] inputTokens[0] must be USDC (pool coins[0])"
    );
    assert_eq!(
        f["inputTokens"][1]["asset"]["address"].as_str(),
        Some("0xf939e0a03fb07f59a73314e73794be0e57ac1b4e"),
        "[rt12] inputTokens[1] must be crvUSD (pool coins[1])"
    );
}

const CD13: &str = "0x1a4d01d20000000000000000000000000000000000000000000000045e92fc05f3dc5dde00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004ec6f43";
#[test]
fn rt13_ng_crvusd_usdc_remove_one_coin() {
    // Part 7 F3 — remove_liquidity_one_coin 0x1a4d01d2. Previously UNCOVERED
    // (bundle had receiver-variant 0x081579a5). gap→covered = F3 fix evidence.
    // outputTokens uses select_from_literal_array (F2-corrected [USDC, crvUSD]).
    check_covered(&Sample {
        label: "crvUSD/USDC NG pool remove_liquidity_one_coin (F3 fix)",
        chain_id: 1,
        to: "0x4dece678ceceb27446b35c672dc7d61f30bad69e",
        selector: "0x1a4d01d2",
        from: "0xb5213b7b98f7d9d4b51dcdb77edc044f90439577",
        value: "0",
        calldata: CD13,
    });
}

const CD14: &str = "0x3df021240000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000065a4da25d3016c0000000000000000000000000000000000000000000000000000000000006fa5f5a95";
#[test]
fn rt14_ng_crvusd_usdc_exchange() {
    // Part 6 F2 — crvUSD/USDC NG pool exchange. calldata CD14: i=1, j=0.
    // On-chain coins() = [USDC(0), crvUSD(1)] → input=coins[1]=crvUSD,
    // output=coins[0]=USDC (real tx: 30,000 crvUSD in → 29,970 USDC out).
    // Pre-fix the bundle coin array was reversed → tokens swapped in the
    // envelope. `check_swap` guards the fix.
    check_swap(
        &Sample {
            label: "crvUSD/USDC NG pool exchange (F2 fix)",
            chain_id: 1,
            to: "0x4dece678ceceb27446b35c672dc7d61f30bad69e",
            selector: "0x3df02124",
            from: "0x5d1124fb77c539ec92e3ef853053bbce1e98271b",
            value: "0",
            calldata: CD14,
        },
        "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e", // crvUSD — input (coins[1])
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC — output (coins[0])
    );
}

const CD15: &str = "0xbc61ea2300000000000000000000000000000000000000000000000000000000000f424000000000000000000000000000000000000000000000005150ae84a8cdf00000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000001eaab18c70e23331857aa47701bb516590c8ba2a00000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000001c1aef";
#[test]
fn rt15_crvusd_create_loan_extended() {
    // Part 7 F6 — create_loan_extended emits a `collateralAmount` leg. CD15 args
    // (cast abi-decode "create_loan_extended(uint256,uint256,uint256,address,uint256[])"):
    // collateral=1000000, debt=1.5e21, N=10. wBTC has 8 decimals → 0.01 wBTC.
    let s = Sample {
        label: "crvUSD wBTC Controller create_loan_extended (F6 fix)",
        chain_id: 1,
        to: "0x4e59541306910ad6dc1dac0ac9dfb29bd9f15c67",
        selector: "0xbc61ea23",
        from: "0x5a722e6f5251881dcdc6a88f18d76bc34e4ced2e",
        value: "0",
        calldata: CD15,
    };
    let v = route(&s);
    println!(
        "\n=== [F6 create_loan_extended] {} ===\n  {}",
        s.label,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(v["ok"], true, "[rt15] expected a declarative HIT: {v}");
    let f = &v["data"]["envelopes"][0]["fields"];
    assert_eq!(
        f["collateralAmount"]["value"].as_str(),
        Some("1000000"),
        "[rt15] collateralAmount.value must be the create_loan_extended collateral arg"
    );
}

const CD16: &str = "0x1e0cfcef";
#[test]
fn rt16_crvusd_unknown_0arg() {
    check_gap(&Sample {
        label: "crvUSD wBTC Controller 0-arg selector 0x1e0cfcef - UNCOVERED",
        chain_id: 1,
        to: "0x4e59541306910ad6dc1dac0ac9dfb29bd9f15c67",
        selector: "0x1e0cfcef",
        from: "0x54072503e6dc5a43d863338d79af4b9a47483950",
        value: "0",
        calldata: CD16,
    });
}

const CD17: &str = "0x65fc3873000000000000000000000000000000000000000000000000d23bf34dd391f1370000000000000000000000000000000000000000000000000000000071945bab";
#[test]
fn rt17_vecrv_create_lock() {
    // Part 7 F7 — create_lock. Previously emitted a `stake` (generic LST)
    // envelope that dropped `_unlock_time`. After F7: a `lock_create` envelope
    // carrying `unlockTime` (absolute epoch) — a 1-week vs 4-year lock distinct.
    let s = Sample {
        label: "veCRV create_lock (F7 fix)",
        chain_id: 1,
        to: "0x5f3b5dfeb7b28cdbd7faba78963ee202a494e2a2",
        selector: "0x65fc3873",
        from: "0x1698f8d2f58c76b3ba591eca2ea934adfd17ff78",
        value: "0",
        calldata: CD17,
    };
    let v = route(&s);
    println!(
        "\n=== [F7 lock_create] {} ===\n  {}",
        s.label,
        serde_json::to_string_pretty(&v).unwrap()
    );
    assert_eq!(v["ok"], true, "[rt17] expected covered after F7: {v}");
    let env = &v["data"]["envelopes"][0];
    assert_eq!(env["action"].as_str(), Some("lock_create"), "[rt17] action");
    assert_eq!(
        env["fields"]["unlockTime"].as_str(),
        Some("1905548203"),
        "[rt17] unlockTime = _unlock_time (absolute epoch, 0x71945bab)"
    );
}

const CD18: &str = "0x2b6e993a0000000000000000000000000000000000000000000000000000000135f1b40000000000000000000000000000000000000000000000000000000000006100ca0000000000000000000000000000000000000000000000001ef28d2f591f0000000000000000000000000000000000000000000000000000794d4ee9b33881af0000000000000000000000000000000000000000000000000000000000000001";
#[test]
fn rt18_tricryptousdc_exchange() {
    check_gap(&Sample {
        label: "tricryptoUSDC exchange 0x2b6e993a - UNCOVERED variant",
        chain_id: 1,
        to: "0x7f86bf177dd4f3494b841a37e810a34dd56c829b",
        selector: "0x2b6e993a",
        from: "0x0dd16c537cdb346826203f3ab762030e7f20c78a",
        value: "2230000000000000000",
        calldata: CD18,
    });
}

const CD19: &str = "0x3df02124000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002710000000000000000000000000000000000000000000000000001ff8205e4d10ea";
#[test]
fn rt19_stableswap_3pool_exchange() {
    check_covered(&Sample {
        label: "3pool exchange (int128 indices)",
        chain_id: 1,
        to: "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7",
        selector: "0x3df02124",
        from: "0xf5e6cbce11e42bd1b3f748f0a5e5f2e1d2758cc4",
        value: "0",
        calldata: CD19,
    });
}

const CD20: &str = "0xecb586a5000000000000000000000000000000000000000000000000085e35f350e93102000000000000000000000000000000000000000000000000017cac4274f23ae0000000000000000000000000000000000000000000000000000000000001a7d900000000000000000000000000000000000000000000000000000000000609b7";
#[test]
fn rt20_stableswap_3pool_remove_liquidity() {
    check_covered(&Sample {
        label: "3pool remove_liquidity uint256-3",
        chain_id: 1,
        to: "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7",
        selector: "0xecb586a5",
        from: "0x8d717ee5ea7e393232da4eb3a150303889c3caf9",
        value: "0",
        calldata: CD20,
    });
}

const CD21: &str = "0x1a4d01d20000000000000000000000000000000000000000000000010ea2723689f391c800000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000001353887";
#[test]
fn rt21_stableswap_3pool_remove_one_coin() {
    check_covered(&Sample {
        label: "3pool remove_liquidity_one_coin",
        chain_id: 1,
        to: "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7",
        selector: "0x1a4d01d2",
        from: "0x230e7f448f6ebc7c4433cce03ee92eeb81681a2c",
        value: "0",
        calldata: CD21,
    });
}

const CD22: &str = "0x4515cef3000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003d090000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003522b9c527455cae";
#[test]
fn rt22_stableswap_3pool_add_liquidity() {
    check_covered(&Sample {
        label: "3pool add_liquidity uint256-3",
        chain_id: 1,
        to: "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7",
        selector: "0x4515cef3",
        from: "0xbcfe1094e2465117f3b3e6b1a15f28d6fc4bb2f1",
        value: "0",
        calldata: CD22,
    });
}

const CD23: &str = "0x6a62784200000000000000000000000095f00391cb5eebcd190eb58728b4ce23dbfa6ac1";
#[test]
fn rt23_minter_mint() {
    check_covered(&Sample {
        label: "Minter mint",
        chain_id: 1,
        to: "0xd061d61a4d941c39e5453435b6345dc261c2fce0",
        selector: "0x6a627842",
        from: "0xb5213b7b98f7d9d4b51dcdb77edc044f90439577",
        value: "0",
        calldata: CD23,
    });
}

const CD24: &str = "0x1e83409a0000000000000000000000002704389cd4c9c4c5865a3ee4b76977b480480004";
#[test]
fn rt24_feedist_crvusd_claim() {
    check_covered(&Sample {
        label: "FeeDistributor crvUSD claim-address",
        chain_id: 1,
        to: "0xd16d5ec345dd86fb63c6a9c43c517210f1027914",
        selector: "0x1e83409a",
        from: "0x2704389cd4c9c4c5865a3ee4b76977b480480004",
        value: "0",
        calldata: CD24,
    });
}

const CD25: &str = "0x394747c50000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000016b28c0000000000000000000000000000000000000000000000000000000000000754a0000000000000000000000000000000000000000000000000000000000000001";
#[test]
fn rt25_cryptoswap_tricrypto2_exchange() {
    check_covered(&Sample {
        label: "tricrypto2 exchange",
        chain_id: 1,
        to: "0xd51a44d3fae010294c616388b506acda1bfaae46",
        selector: "0x394747c5",
        from: "0x3c33b8631c24b870be93df828ba7bc5100aec795",
        value: "0",
        calldata: CD25,
    });
}

const CD26: &str = "0x3df021240000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b1a5bc74713afb00000000000000000000000000000000000000000000000000b0b9515e6ad029";
#[test]
fn rt26_stableswap_steth_exchange() {
    check_covered(&Sample {
        label: "stETH pool exchange",
        chain_id: 1,
        to: "0xdc24316b9ae028f1497c275eb9192a3ea0f67022",
        selector: "0x3df02124",
        from: "0x77d546bf921ef46c33b72b71fc2c91f54106d2c7",
        value: "0",
        calldata: CD26,
    });
}

const CD27: &str = "0x0b4c7e4d00000000000000000000000000000000000000000000000022b1c8c1227a000000000000000000000000000000000000000000000000000022b1c8c1227a00000000000000000000000000000000000000000000000000003cf7698be2c630ed";
#[test]
fn rt27_stableswap_steth_add_liquidity() {
    check_covered(&Sample {
        label: "stETH pool add_liquidity uint256-2",
        chain_id: 1,
        to: "0xdc24316b9ae028f1497c275eb9192a3ea0f67022",
        selector: "0x0b4c7e4d",
        from: "0xc60b9019bbccf651855e2e208429a2ec5bcf2e63",
        value: "2500000000000000000",
        calldata: CD27,
    });
}

const CD28: &str = "0x6e553f650000000000000000000000000000000000000000000003a0d70f1bafd9a650480000000000000000000000002e89a9e33feae350311c943d8644b8288270c99b";
#[test]
fn rt28_scrvusd_deposit() {
    check_gap(&Sample {
        label: "scrvUSD ERC-4626 deposit - UNCOVERED",
        chain_id: 1,
        to: "0x0655977feb2f289a4ab78af67bab0d17aab84367",
        selector: "0x6e553f65",
        from: "0x2e89a9e33feae350311c943d8644b8288270c99b",
        value: "0",
        calldata: CD28,
    });
}

const CD29: &str = "0x23cfed03000000000000000000000000000000000000000000000000000000000002659c000000000000000000000000000000000000000000000002b5e3af16b18800000000000000000000000000000000000000000000000000000000000000000032";
#[test]
fn rt29_curve_lending_create_loan() {
    check_gap(&Sample {
        label: "Curve Lending controller create_loan - UNCOVERED",
        chain_id: 1,
        to: "0xcad85b7fe52b1939dceebee9bcf0b2a5aa0ce617",
        selector: "0x23cfed03",
        from: "0xe2e96eeb4595b3d93904f9b40c7a261838cc2459",
        value: "0",
        calldata: CD29,
    });
}
