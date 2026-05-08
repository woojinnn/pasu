//! Enrichment utilities that stamp optional policy context before evaluation.

mod dex;
pub mod signature;

pub use dex::{
    compute_dex_window_deltas, enrich_dex_action, enrich_dex_action_base, enrich_dex_window_stats,
};
pub use signature::enrich_signature_action;
