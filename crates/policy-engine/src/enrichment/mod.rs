//! Host-driven enrichment for normalized action envelopes.
//!
//! Enrichment is additive and best-effort: missing host capabilities or host
//! errors leave the corresponding optional action fields unset.

pub use dispatch::enrich_envelope;

mod actions;
mod dispatch;
pub(crate) mod usd;
