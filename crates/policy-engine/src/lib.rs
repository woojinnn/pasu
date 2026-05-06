//! Web3 wallet transaction policy engine — runtime crate.
//!
//! This crate hosts the pieces an end-to-end pipeline needs at runtime:
//!
//! - **`core`**: shared domain types (`Address`, `Token`, `TransactionRequest`,
//!   `Action`, `AmountSpec`, `UsdValuation`).
//! - **`host`**: host-provided capabilities (`Oracle`, `Portfolio`, `Approvals`,
//!   `StatWindows`) shared by adapters and pipeline.
//! - **`policy`**: `PolicyEngine` (Cedar wrapper) and the `PolicyRequest`
//!   shape that adapters produce and the engine consumes.
//! - **`adapter`**: the `Adapter` trait + adapter ids/errors/match keys.
//!   Concrete adapter implementations live in *their own crates* under
//!   `crates/adapters/<name>/`.
//! - **`registry`**: adapter resolution traits and the in-memory registry.
//! - **`lowering`**: `Action` enrichment and `PolicyRequest` construction.
//! - **`prelude`**: the curated import surface adapter authors use
//!   (`use policy_engine::prelude::*;`).
//! - **`pipeline`**: the orchestrator that wires resolver → adapter →
//!   oracle-enrichment → Cedar evaluator together.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::todo)]
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic))]
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub mod adapter;
pub mod context_keys;
pub mod core;
pub mod host;
pub mod lowering;
pub mod pipeline;
pub mod policy;
pub mod prelude;
pub mod registry;
pub mod schema;

pub use adapter::{
    ActionKind, Adapter, AdapterDescriptor, AdapterError, AdapterFactory, AdapterId, AdapterKind,
    ContractTarget, MatchKey, SolidityFunction, SolidityFunctionSpec, StaticAdapterFactory,
    TypedAdapter,
};
pub use core::{
    Action, Address, AmountSpec, ChainId, DexAction, DexFacts, DexTrace, OracleRequirement,
    OracleRequirementKind, OtherAction, Token, TransactionRequest, UsdValuation,
    WindowStatsContext,
};
pub use host::{
    Approvals, ApprovalsError, HostCapabilities, MockApprovals, MockOracle, MockPortfolio,
    MockStatWindows, Oracle, OracleError, Portfolio, PortfolioError, ReservationId, StatDelta,
    StatKey, StatValue, StatWindows,
};
pub use lowering::{
    compute_dex_window_deltas, enrich_dex_action, enrich_dex_action_base, enrich_dex_window_stats,
    request_from_action, requests_from_action, requests_from_actions,
};
pub use pipeline::{EvaluationOutcome, Pipeline, PipelineError};
pub use policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyError, PolicyRequest, RequestKind,
    Severity, Verdict,
};
pub use registry::{AdapterIndex, AdapterRegistry, MockAdapterRegistry, ResolverOutcome};
pub use schema::PolicySchemaComposer;
