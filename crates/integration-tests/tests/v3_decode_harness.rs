//! Deterministic CI gate for the v3 `ActionBody[]` decode harness.
//!
//! 1. `surface_installs_clean` — every local `registryV2/index` bundle installs
//!    into the WASM v3 state without error.
//! 2. `synthetic_fuzz_single_emit` — fuzzing every `single_emit` callkey with a
//!    fixed seed produces zero hard failures (no panic, no serde-shape break,
//!    no hard engine error).
//!
//! Both install + route on their own test thread (R1: WASM v3 install state is
//! thread-local). Phase 2/3 add `synthetic_fuzz_all_strategies` and
//! `corpus_replay` to this file.

use policy_engine_integration_tests::harness::{self, adapters};

/// Fixed seed for the deterministic sweep (valid hex; arbitrary).
const GLOBAL_SEED: u64 = 0x5C09_EBA1;
/// Iterations per callkey in the gate (kept small for speed; CLI raises it).
const ITERS_PER_CALLKEY: u64 = 16;

#[test]
fn surface_installs_clean() {
    let surface = adapters::load_and_install().expect("load + install local registryV2 index");

    eprintln!(
        "callkeys={} typed_data_keys={} unique_bundles_installed={} install_failures={}",
        surface.total_callkeys,
        surface.total_typed_keys,
        surface.installed_bundle_ids.len(),
        surface.install_failures.len(),
    );
    for (id, err) in &surface.install_failures {
        eprintln!("  INSTALL FAIL {id}: {err}");
    }

    assert!(
        surface.install_failures.is_empty(),
        "{} local bundle(s) failed to install",
        surface.install_failures.len(),
    );
    // Loose staleness guard: the index is committed; an empty/broken index means
    // build-index.ts needs rerunning. (Exact counts churn with the token list.)
    assert!(
        surface.total_callkeys >= 300,
        "index looks stale/empty ({} callkeys) — run `npx tsx registryV2/scripts/build-index.ts`",
        surface.total_callkeys,
    );
    assert!(
        surface.installed_bundle_ids.len() >= 50,
        "too few unique bundles installed ({})",
        surface.installed_bundle_ids.len(),
    );
}

#[test]
fn synthetic_fuzz_single_emit() {
    let report = harness::run_synthetic_single_emit(GLOBAL_SEED, ITERS_PER_CALLKEY)
        .expect("run synthetic single_emit fuzz");

    eprintln!("{}", report.summary());

    assert_eq!(
        report.hard_failures(),
        0,
        "synthetic single_emit fuzz found {} hard failure(s):\n{}",
        report.hard_failures(),
        report.summary(),
    );
}

#[test]
fn corpus_replay() {
    let root = harness::default_corpus_root();
    let outcomes = harness::corpus::run_corpus(&root).expect("run real-tx corpus");

    let mismatches: Vec<_> = outcomes.iter().filter(|o| !o.matched).collect();
    eprintln!(
        "corpus: {}/{} matched",
        outcomes.len() - mismatches.len(),
        outcomes.len()
    );
    for m in &mismatches {
        eprintln!(
            "  MISS [{}] {} expect={} got={}",
            m.source, m.label, m.expect, m.got
        );
    }

    assert!(
        !outcomes.is_empty(),
        "no corpus.json found under {}",
        root.display()
    );
    assert!(
        mismatches.is_empty(),
        "{} corpus entr(ies) did not match their pinned expectation",
        mismatches.len()
    );
}

#[test]
fn synthetic_fuzz_all_strategies() {
    let report = harness::run_synthetic_all(GLOBAL_SEED, ITERS_PER_CALLKEY)
        .expect("run synthetic all-strategy fuzz");

    eprintln!("{}", report.summary());

    assert_eq!(
        report.hard_failures(),
        0,
        "synthetic all-strategy fuzz found {} hard failure(s):\n{}",
        report.hard_failures(),
        report.summary(),
    );
}

/// Recursively find the first `"<field>": "<string>"` entry in a JSON value.
fn find_string_field(v: &serde_json::Value, field: &str) -> Option<String> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::String(s)) = m.get(field) {
                return Some(s.clone());
            }
            m.values().find_map(|x| find_string_field(x, field))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_string_field(x, field)),
        _ => None,
    }
}

/// Field-level golden (manual oracle layer "A", manual §5c) for the Morpho Blue
/// `market_id`.
///
/// `corpus_replay`'s oracle (`corpus.rs::check_expect`) compares only the
/// verdict + top-level `domain` — it never inspects body field VALUES. So a
/// Morpho `supply` whose `market_id` is wrong (say, naively mapped to a plain
/// `$args.*` field instead of the keccak, or with the Tier B injector removed)
/// would still pass `corpus_replay` as `pass`/`lending` — a SILENT mis-decode.
/// This test is the only thing that pins it: it routes a real mainnet supply tx
/// and asserts the decoded `LendingVenue::MorphoBlue.market_id` equals
/// `keccak256(abi.encode(marketParams))` (= `MarketParamsLib.id`), the value
/// `maybe_inject_morpho_market_id` must produce.
#[test]
fn morpho_supply_market_id_is_keccak_marketparams() {
    // R1: install + route on the same thread.
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet supply tx 0xf2cdff2b1203…: market (loan=WETH 0xC02a…,
    // collat=0xe1B4…, oracle=0xcb6a…, irm=0x870a…, lltv=91.5%), 2.5157 WETH.
    const TO: &str = "0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb";
    const CALLDATA: &str = "0xa99aad89000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000e1b4d34e8754600962cd944b535180bd758e6c2e000000000000000000000000cb6a6fdfdb18ec9a004465aef74ff9092fd4f89a000000000000000000000000870ac11d48b15db9a138cf899d20f13f79ba00bc0000000000000000000000000000000000000000000000000cb2bba6f17b800000000000000000000000000000000000000000000000000022e9df45f93190e8000000000000000000000000000000000000000000000000000000000000000000000000000000000000000040534e513df8277870b81e97b5107b3f39de4f1500000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000000";

    // keccak256(abi.encode(marketParams)) = MarketParamsLib.id. Cross-checked
    // independently of the Rust injector via
    //   cast abi-encode "f((address,address,address,address,uint256))" "(…)" | cast keccak
    const EXPECTED_MARKET_ID: &str =
        "0xb7ad412532006bf876534ccae59900ddd9d1d1e394959065cb39b12b22f94ff5";

    let env = harness::route::route_calldata(1, TO, "0xa99aad89", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let market_id =
        find_string_field(&env, "market_id").expect("decoded supply body carries a market_id");
    assert_eq!(
        market_id, EXPECTED_MARKET_ID,
        "Morpho market_id mismatch — Tier B keccak(MarketParams) regressed"
    );
}

/// Recursively find the first `"<field>": <bool>` entry in a JSON value.
fn find_bool_field(v: &serde_json::Value, field: &str) -> Option<bool> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::Bool(b)) = m.get(field) {
                return Some(*b);
            }
            m.values().find_map(|x| find_bool_field(x, field))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_bool_field(x, field)),
        _ => None,
    }
}

/// Field-level golden for Morpho `setAuthorization` (Tier 3
/// `LendingAction::SetAuthorization`).
///
/// The corpus oracle checks only the verdict + top-level domain — never WHO is
/// being authorized. So a manifest that mis-maps `authorized` (e.g. to the
/// protocol address or a wrong arg) would still pass as `pass`/`lending`. This
/// pins the operator address + grant flag from a real mainnet `setAuthorization`
/// tx — the security-critical fields for a permission-delegation analyzer.
#[test]
fn morpho_set_authorization_decodes_operator_and_flag() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet setAuthorization tx 0x255f24ea…: grant control to the
    // operator 0x4A6c312e… (newIsAuthorized = true).
    const TO: &str = "0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb";
    const CALLDATA: &str = "0xeecea0000000000000000000000000004a6c312ec70e8747a587ee860a0353cd42be0ae00000000000000000000000000000000000000000000000000000000000000001";

    let env = harness::route::route_calldata(1, TO, "0xeecea000", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let authorized =
        find_string_field(&env, "authorized").expect("set_authorization body carries `authorized`");
    assert_eq!(
        authorized, "0x4a6c312ec70e8747a587ee860a0353cd42be0ae0",
        "operator (authorized) address mis-decoded"
    );
    assert_eq!(
        find_bool_field(&env, "is_authorized"),
        Some(true),
        "grant flag (is_authorized) mis-decoded"
    );
}
