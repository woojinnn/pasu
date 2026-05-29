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
