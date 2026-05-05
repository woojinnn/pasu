//! Web3 wallet transaction policy engine — runtime crate.
//!
//! This crate hosts the pieces an end-to-end pipeline needs at runtime:
//!
//! - **`core`**: shared domain types (`Address`, `Token`, `TransactionRequest`,
//!   `Action`, `AmountSpec`, `UsdValuation`).
//! - **`oracle`**: the `Oracle` trait + the in-memory `MockOracle` used by
//!   tests and the playground. Real-API implementations (e.g., HTTP-backed
//!   oracles) are expected to live in their own crates and implement `Oracle`.
//! - **`policy`**: `PolicyEngine` (Cedar wrapper) and the `PolicyRequest`
//!   shape that adapters produce and the engine consumes.
//! - **`adapter`**: the `Adapter` trait + adapter ids/errors/match keys.
//!   Concrete adapter implementations live in *their own crates* under
//!   `crates/adapters/<name>/`.
//! - **`registry`**: adapter resolution traits and the in-memory registry.
//! - **`lowering`**: `Action`/`Action::Multi` expansion, USD enrichment, and
//!   `PolicyRequest` construction.
//! - **`prelude`**: the curated import surface adapter authors use
//!   (`use policy_engine::prelude::*;`).
//! - **`pipeline`**: the orchestrator that wires resolver → adapter →
//!   oracle-enrichment → Cedar evaluator together.

pub mod adapter;
pub mod core;
pub mod lowering;
pub mod oracle;
pub mod pipeline;
pub mod policy;
pub mod prelude;
pub mod registry;

pub use adapter::{
    ActionKind, Adapter, AdapterDescriptor, AdapterError, AdapterFactory, AdapterId, AdapterKind,
    ContractTarget, MatchKey, SolidityFunction, SolidityFunctionSpec, StaticAdapterFactory,
    TypedAdapter,
};
pub use core::{
    Action, Address, AmountSpec, ChainId, MultiAction, SwapAction, Token, TransactionRequest,
    UsdValuation,
};
pub use lowering::{
    enrich_actions_with_usd, enrich_with_usd, request_from_action, requests_from_action,
    requests_from_actions,
};
pub use oracle::{MockOracle, Oracle, OracleError};
pub use pipeline::{Pipeline, PipelineError};
pub use policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyError, PolicyRequest, Severity, Verdict,
};
pub use registry::{AdapterIndex, AdapterRegistry, MockAdapterRegistry, ResolverOutcome};
