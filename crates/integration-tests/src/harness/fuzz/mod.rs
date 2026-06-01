//! Strategy-aware synthetic fuzzers.
//!
//! Each `emit.strategy` needs different calldata construction:
//! * [`single_emit`] — flat ABI args (the bulk of the surface).
//! * `opcode_stream` / `array_emit` / `tagged_dispatch` / `typed_data` — added
//!   in Phase 2.
//!
//! All share [`values`] (ABI-type → `DynSolValue` generation, seeded + edge).

pub mod opcode_stream;
pub mod single_emit;
pub mod tagged_dispatch;
pub mod typed_data;
pub mod values;

use crate::harness::adapters::RoutableSurface;
use crate::harness::report::Report;

/// Iterations per callkey, in a single deterministic test sweep, that emit
/// boundary (`Edge`) values before switching to random. Keep small so the CI
/// gate stays fast; the CLI raises the total via `--iterations`.
pub const EDGE_ITERS: u64 = 4;

/// Run every strategy fuzzer over the surface into one report.
pub fn fuzz_all(surface: &RoutableSurface, global_seed: u64, iters: u64, report: &mut Report) {
    single_emit::fuzz_surface(surface, global_seed, iters, report);
    opcode_stream::fuzz_surface(surface, global_seed, iters, report);
    tagged_dispatch::fuzz_surface(surface, global_seed, iters, report);
    typed_data::fuzz_surface(surface, global_seed, iters, report);
}
