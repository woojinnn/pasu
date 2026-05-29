//! v3 `ActionBody[]` decode harness.
//!
//! Drives the **production** decode entrypoints
//! (`policy_engine_wasm::declarative_{install,route_request,route_typed_data}_v3_json`)
//! — the same Rust source that ships as WASM — across the full local adapter
//! surface (`registryV2/index/`), with no browser, no WASM runtime, and no RPC
//! server. Two front-ends consume this engine:
//!
//! * the `v3_decode_harness` cargo test (deterministic CI gate), and
//! * the `v3-harness` CLI binary (unbounded fuzzing + reporting).
//!
//! The harness verifies that `ActionBody[]` is produced *correctly* — structure
//! only, since `live_inputs.value` is intentionally empty until the sync
//! orchestrator is wired. See the module-level docs of [`oracle`] for what
//! "correctly" means.
//!
//! ## Thread-locality (R1)
//! The WASM v3 install state is a thread-local. Install and route must happen on
//! the **same OS thread**. [`adapters::RoutableSurface`] installs into the
//! current thread; every front-end routes on that same thread.

// Harness code is a test tool, not shipped production logic. The crate-wide
// pedantic/nursery lints are too noisy here; keep the substantive lints
// (missing_docs, unsafe, unused) and silence the stylistic ones.
#![allow(clippy::pedantic, clippy::nursery, clippy::missing_errors_doc)]

pub mod adapters;
pub mod corpus;
pub mod encode;
pub mod fuzz;
pub mod oracle;
pub mod prng;
pub mod report;
pub mod route;

use anyhow::Result;

/// Default real-tx corpus root: `<crate>/data/golden/v3-decode`.
#[must_use]
pub fn default_corpus_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/golden/v3-decode")
}

/// Run a synthetic `single_emit` fuzz sweep over the whole local surface.
///
/// Installs all local adapters into the (thread-local) WASM state, then fuzzes
/// every `single_emit` callkey `iters` times with a fixed `global_seed`. Returns
/// the aggregated [`report::Report`]. Install + route happen on the calling
/// thread (R1).
pub fn run_synthetic_single_emit(global_seed: u64, iters: u64) -> Result<report::Report> {
    let surface = adapters::load_and_install()?;
    let mut report = report::Report::default();
    with_silenced_panics(|| {
        fuzz::single_emit::fuzz_surface(&surface, global_seed, iters, &mut report);
    });
    Ok(report)
}

/// Run **all** strategy fuzzers (single_emit + opcode_stream + tagged_dispatch +
/// typed_data) over the whole local surface with a fixed `global_seed`.
pub fn run_synthetic_all(global_seed: u64, iters: u64) -> Result<report::Report> {
    let surface = adapters::load_and_install()?;
    let mut report = report::Report::default();
    with_silenced_panics(|| {
        fuzz::fuzz_all(&surface, global_seed, iters, &mut report);
    });
    Ok(report)
}

/// Replay one `single_emit` callkey at a fixed seed, returning the raw route
/// envelope (`{ok, data, error}`). Used by the CLI `replay` command to
/// reproduce a fuzz failure. Non-`single_emit` strategies are not replayable
/// standalone yet (use corpus replay).
pub fn replay(callkey: &str, seed: u64) -> Result<serde_json::Value> {
    let surface = adapters::load_and_install()?;
    let call = surface
        .calls
        .iter()
        .find(|c| c.source_callkey == callkey)
        .ok_or_else(|| anyhow::anyhow!("callkey not found on surface: {callkey}"))?;
    if call.strategy == adapters::Strategy::SingleEmit
        && !call.has_typed_data
        && call.selector != "0x00000000"
    {
        let calldata = fuzz::single_emit::build_calldata(call, seed, fuzz::values::Edge::Random)?;
        Ok(route::route_calldata(
            call.chain_id,
            &call.to,
            &call.selector,
            &calldata,
            "0",
        ))
    } else {
        Err(anyhow::anyhow!(
            "replay supports single_emit calldata only; `{callkey}` is strategy={} (use corpus replay)",
            call.strategy.as_str()
        ))
    }
}

/// Run `f` with the panic hook silenced (so per-iteration `catch_unwind`
/// recoveries don't spam stderr), restoring the previous hook afterwards.
pub fn with_silenced_panics<R>(f: impl FnOnce() -> R) -> R {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = f();
    std::panic::set_hook(prev);
    out
}
