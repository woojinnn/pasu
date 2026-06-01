//! Runtime orchestration.
//!
//! This layer wires source adapters, the `LiveField` pipeline, primitive sync,
//! and polling scheduler into the API used by the policy server.

pub mod config;
pub mod error;
pub mod orchestrator;
pub mod scheduler;
