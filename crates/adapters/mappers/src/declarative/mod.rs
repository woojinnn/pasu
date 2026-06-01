//! Declarative DSL (Tier A) for adapter loader — v3 builder surface.
//!
//! Phase 1 (action-types) trimmed this module to the v3 path. The legacy v1
//! interpreter (`eval`, `builtin_fn`, `single_emit`, `multicall`,
//! `opcode_stream`, `enum_tagged`, `array_emit`, `mapper::DeclarativeMapper`)
//! has been removed alongside the v1 `Mapper` trait.
//!
//! Module layout:
//! ```text
//!   types.rs          — Bundle JSON struct/enum (serde Deserialize / Serialize)
//!   args_json.rs      — DecodedCall → serde_json args view (v1-free)
//!   action_builder.rs — body/placeholder template → policy-transition ActionBody (v3)
//! ```

pub mod action_builder;
pub mod args_json;
pub mod builtin_fn;
pub mod types;

pub use args_json::{args_to_json, decoded_value_to_json};
