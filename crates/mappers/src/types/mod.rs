//! Rust types mirroring `schema_demo/schema/*.json`. Each module corresponds
//! to one JSON Schema file:
//!
//! - `common.rs`   ↔  `schema/common/_common.json`
//! - `actions.rs`  ↔  `schema/actions/{swap,wrap,unwrap,approve,...}.json`
//! - `envelope.rs` ↔  inline `$defs/ActionEnvelope` in `schema/root.json`
//! - `root.rs`     ↔  `schema/root.json`

pub mod actions;
pub mod common;
pub mod envelope;
pub mod root;

pub use actions::*;
pub use common::*;
pub use envelope::*;
pub use root::*;
