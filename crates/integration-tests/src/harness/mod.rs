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
pub mod projection;
pub mod report;
pub mod route;
pub mod semantic;

use std::collections::HashSet;
use std::sync::Arc;

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
    run_synthetic_all_filtered(global_seed, iters, None)
}

/// Run all strategy fuzzers over a filtered local surface.
///
/// `filter`: case-insensitive substring matched against callkeys, typed-data
/// keys, or bundle ids. This keeps protocol onboarding fuzz runs scoped and
/// replayable without changing the registry surface that gets installed.
pub fn run_synthetic_all_filtered(
    global_seed: u64,
    iters: u64,
    filter: Option<&str>,
) -> Result<report::Report> {
    let surface = adapters::load_and_install()?;
    let surface = filter_surface(surface, filter);
    let mut report = report::Report::default();
    with_silenced_panics(|| {
        fuzz::fuzz_all(&surface, global_seed, iters, &mut report);
    });
    Ok(report)
}

fn filter_surface(
    surface: Arc<adapters::RoutableSurface>,
    filter: Option<&str>,
) -> Arc<adapters::RoutableSurface> {
    let Some(filter) = filter else {
        return surface;
    };
    // Only the (rare) protocol-scoped fuzz path clones the shared surface to
    // prune it; the common unfiltered path returns the cached `Arc` untouched.
    let filter = filter.to_ascii_lowercase();
    let mut surface = (*surface).clone();
    surface.calls.retain(|c| {
        c.source_callkey.contains(&filter) || c.bundle_id.to_ascii_lowercase().contains(&filter)
    });
    surface.typed.retain(|t| {
        t.source_key.to_ascii_lowercase().contains(&filter)
            || t.bundle_id.to_ascii_lowercase().contains(&filter)
    });
    surface.total_callkeys = surface.calls.len();
    surface.total_typed_keys = surface.typed.len();
    surface.installed_bundle_ids.retain(|id| {
        surface.calls.iter().any(|c| c.bundle_id == *id)
            || surface.typed.iter().any(|t| t.bundle_id == *id)
    });
    Arc::new(surface)
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

/// Author-time structural validation options.
#[derive(Clone, Copy, Debug, Default)]
pub struct ValidateOptions {
    /// For `3-ref` source-generated surfaces, validate one callkey per source
    /// bundle template instead of every materialized address. This keeps CI
    /// bounded for pool-heavy protocols while preserving `validate`'s exhaustive
    /// mode for local/nightly gates.
    pub representative_source_refs: bool,
}

/// One adapter's author-time structural-validation verdict.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ManifestVerdict {
    /// Owning bundle id (== the manifest id, e.g. `curve/stableswap-ng/2btc/exchange@1.0.0`).
    pub bundle_id: String,
    /// Routing key the verdict was produced from.
    pub callkey: String,
    /// `true` = every synthesized input decoded to a well-formed `ActionBody`.
    pub ok: bool,
    /// First hard oracle failure (`<layer>: <detail>`), if any. For a body-shape
    /// bug this is input-independent (e.g. `ErrorClass: build_action_body_failed:
    /// missing field \`live_inputs\``).
    pub error: Option<String>,
    /// Seed that reproduced the failure (`replay --seed`). Shape bugs reproduce on
    /// any input, so the printed repro reproduces them regardless of edge/random.
    pub seed: Option<u64>,
}

/// **Author-time `emit.body` shape validator** (the build-index header's promised
/// `validate-emit-body` step, realized against the production decoder).
///
/// For each (optionally `filter`-matched) `single_emit` adapter, synthesize
/// `iters` ABI-typed inputs from its `abi_fragment`, route each through the
/// production decoder, and judge the envelope. A manifest **fails** if ANY
/// iteration yields a hard oracle failure — i.e. the `emit.body` template does
/// not match the typed `ActionBody` struct (missing/renamed field, unknown
/// variant, wrong venue/param shape, domain drift). Input-dependent artifacts
/// (`value-map: no case`, array index out of bounds) are oracle-**soft** and
/// never fail here, so fuzzing `$args.i` over an out-of-range coin index does
/// not produce a false positive.
///
/// `filter`: substring matched against `source_callkey` OR `bundle_id`
/// (`None` = whole surface). Reads the built `registryV2/index/` — run
/// `npm run build` (build-index) after authoring, before validating.
pub fn validate(filter: Option<&str>, iters: u64) -> Result<Vec<ManifestVerdict>> {
    validate_with_options(filter, iters, ValidateOptions::default())
}

/// Same as [`validate`], with source-ref sampling controls for CI-sized gates.
pub fn validate_with_options(
    filter: Option<&str>,
    iters: u64,
    options: ValidateOptions,
) -> Result<Vec<ManifestVerdict>> {
    let surface = adapters::load_and_install_with_options(adapters::LoadOptions {
        filter: filter.map(ToOwned::to_owned),
        representative_source_refs: options.representative_source_refs,
        include_typed_data: false,
    })?;
    let mut out = Vec::new();
    let mut seen_source_ref_keys = HashSet::new();
    with_silenced_panics(|| {
        for call in surface.calls.iter().filter(|c| {
            c.strategy == adapters::Strategy::SingleEmit
                && !c.has_typed_data
                && c.selector != "0x00000000"
                && filter.is_none_or(|f| c.source_callkey.contains(f) || c.bundle_id.contains(f))
        }) {
            if options.representative_source_refs {
                if let Some(key) = &call.source_ref_key {
                    if !seen_source_ref_keys.insert(key.clone()) {
                        continue;
                    }
                }
            }
            let base = encode::fnv1a64(&call.source_callkey);
            let mut verdict = ManifestVerdict {
                bundle_id: call.bundle_id.clone(),
                callkey: call.source_callkey.clone(),
                ok: true,
                error: None,
                seed: None,
            };
            for i in 0..iters {
                let seed = base ^ i;
                let edge = if i < fuzz::EDGE_ITERS {
                    fuzz::values::Edge::Edge
                } else {
                    fuzz::values::Edge::Random
                };
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fuzz::single_emit::fuzz_one(call, seed, edge)
                }));
                match res {
                    // Type-valid input that decoded clean or only soft-errored: keep going.
                    Ok(Ok((_, judged))) => {
                        if let oracle::Verdict::Fail { layer, detail } = judged.verdict {
                            verdict.ok = false;
                            verdict.error = Some(format!("{layer:?}: {detail}"));
                            verdict.seed = Some(seed);
                            break;
                        }
                    }
                    // Harness could not build args for this ABI (a skip, not a finding).
                    Ok(Err(_)) => {}
                    Err(_) => {
                        verdict.ok = false;
                        verdict.error = Some("route panicked".to_owned());
                        verdict.seed = Some(seed);
                        break;
                    }
                }
            }
            out.push(verdict);
        }
    });
    Ok(out)
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
