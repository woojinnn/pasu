//! `single_emit` fuzzer — flat ABI args → calldata → route → oracle.
//!
//! Covers the bulk of the surface (ERC-standard tokens, Aave Pool, single-fn
//! adapters). Any type-valid input MUST decode to a well-formed `ActionBody`,
//! so a hard engine error here is a finding.

use std::panic::{catch_unwind, AssertUnwindSafe};

use alloy_dyn_abi::DynSolValue;
use anyhow::Result;

use crate::harness::adapters::{RoutableCall, RoutableSurface, Strategy};
use crate::harness::fuzz::values::{abi_input_to_soltype, gen_value, Edge};
use crate::harness::fuzz::EDGE_ITERS;
use crate::harness::oracle::{judge, Judged};
use crate::harness::{encode, route};

/// Build args + calldata for one iteration, route it, and judge the envelope.
/// Returns `(calldata, judged)`, or `Err` if the harness could not build args
/// for this ABI (a harness skip, not a decode finding).
pub fn fuzz_one(call: &RoutableCall, seed: u64, edge: Edge) -> Result<(String, Judged)> {
    let calldata = build_calldata(call, seed, edge)?;
    let env = route::route_calldata(call.chain_id, &call.to, &call.selector, &calldata, "0");
    Ok((calldata, judge(&env)))
}

/// Build the `0x`-prefixed calldata for one fuzz iteration (no routing). Used by
/// [`fuzz_one`] and by the CLI `replay` command.
pub fn build_calldata(call: &RoutableCall, seed: u64, edge: Edge) -> Result<String> {
    let mut rng = crate::harness::prng::SplitMix64::new(seed);
    let args = call
        .abi_inputs
        .iter()
        .map(|i| Ok(gen_value(&mut rng, &abi_input_to_soltype(i)?, edge)))
        .collect::<Result<Vec<DynSolValue>>>()?;
    Ok(encode::encode_calldata(&call.selector, &args))
}

/// Fuzz every `single_emit` callkey on the surface `iters` times each.
///
/// Seed per iteration = `fnv1a64(callkey) ^ global_seed ^ i` (position-stable,
/// replayable). The first [`EDGE_ITERS`] iterations use boundary values.
pub fn fuzz_surface(
    surface: &RoutableSurface,
    global_seed: u64,
    iters: u64,
    report: &mut crate::harness::report::Report,
) {
    // Calldata-routable single_emit only. Entries with `match.typed_data` are
    // sign-primary (sentinel callkey selectors + named EIP-712 message bodies)
    // and are exercised by the typed-data fuzzer (Phase 2) instead.
    for call in surface.calls.iter().filter(|c| {
        c.strategy == Strategy::SingleEmit
            && !c.has_typed_data
            // `0x00000000` is the selector-less / native-transfer (and synthetic)
            // sentinel — it needs empty calldata + value, not generic ABI-encoded
            // calldata. Covered by a dedicated native-transfer path, not here.
            && c.selector != "0x00000000"
    }) {
        let base = encode::fnv1a64(&call.source_callkey);
        for i in 0..iters {
            let seed = base ^ global_seed ^ i;
            let edge = if i < EDGE_ITERS {
                Edge::Edge
            } else {
                Edge::Random
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| fuzz_one(call, seed, edge)));
            match outcome {
                Ok(Ok((calldata, judged))) => report.record(
                    &call.source_callkey,
                    &call.bundle_id,
                    "single_emit",
                    seed,
                    &calldata,
                    &judged,
                ),
                Ok(Err(_)) => report.record_skip(),
                Err(_) => report.record_panic(
                    &call.source_callkey,
                    &call.bundle_id,
                    "single_emit",
                    seed,
                    "<panic before calldata>",
                ),
            }
        }
    }
}
