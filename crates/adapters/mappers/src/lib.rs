//! Calldata → v3 `ActionBody` declarative builder.
//!
//! Phase 1 (action-types) trimmed this crate to the v3 declarative path only.
//! The legacy v1 `Mapper` trait and its per-protocol implementations have been
//! removed. What remains is the
//! self-contained v3 builder consumed by `policy-engine-wasm`'s
//! `declarative_route_request_v3_json`:
//!
//! ```text
//!   DecodedCall (abi-resolver) → declarative::args_json → JSON args view
//!                              → declarative::action_builder → simulation-reducer ActionBody
//! ```
//!
//! Module layout:
//! ```text
//!   declarative/
//!     action_builder.rs - body/placeholder template → ActionBody (v3)
//!     args_json.rs      - DecodedCall → serde_json args view (v1-free)
//!     types.rs          - Bundle JSON struct/enum (serde)
//! ```

#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::private_intra_doc_links)]
#![allow(rustdoc::redundant_explicit_links)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]

pub mod declarative;
