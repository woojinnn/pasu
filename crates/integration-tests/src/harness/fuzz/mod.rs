//! Strategy-aware synthetic fuzzers.
//!
//! Each `emit.strategy` needs different calldata construction:
//! * [`single_emit`] — flat ABI args (the bulk of the surface).
//! * `opcode_stream` / `array_emit` / `tagged_dispatch` / `typed_data` — added
//!   in Phase 2.
//!
//! All share [`values`] (ABI-type → `DynSolValue` generation, seeded + edge).

pub mod single_emit;
pub mod values;

/// Iterations per callkey, in a single deterministic test sweep, that emit
/// boundary (`Edge`) values before switching to random. Keep small so the CI
/// gate stays fast; the CLI raises the total via `--iterations`.
pub const EDGE_ITERS: u64 = 4;
