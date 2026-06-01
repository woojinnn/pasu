//! External source adapters and authoritative primitive sync.
//!
//! `LiveField<T>` is the unified representation for independently stale fields
//! embedded in state or actions. Some wallet primitives are intentionally not
//! modeled as `LiveField`s yet: balances, approvals, block heights, and venue
//! account snapshots are authoritative snapshots that replace primitive state
//! in bulk. Those paths live here next to the lower-level fetchers so the
//! difference is explicit rather than hidden in the runtime orchestrator.

pub mod discovery;
pub mod fetchers;
pub mod primitives;
pub mod subscription;
