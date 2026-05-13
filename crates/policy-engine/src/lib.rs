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
//! - **`adapter`**: the `TransactionActionAdapter` trait + adapter ids/errors/match keys.
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

pub mod action;
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

pub use action::{
    Action, ActionEnvelope, Address as ActionAddress, AmountConstraint, AmountKind, AssetKind,
    AssetRef, Category, DecimalString, Hex, UsdValuation as ActionUsdValuation, Validity,
    ValiditySource,
};
pub use adapter::{
    ActionAdapterError, ActionAdapterId, ActionKind, ContractTarget,
    DeclaredSignatureActionAdapter, DeclaredTransactionActionAdapter, SignatureActionAdapter,
    SignatureActionAdapterDescriptor, SignatureMatchKey, SolidityFunction, SolidityFunctionSpec,
    StaticTransactionActionAdapterFactory, TransactionActionAdapter,
    TransactionActionAdapterDescriptor, TransactionActionAdapterFactory,
    TransactionActionAdapterKind, TransactionMatchKey,
};
pub use core::{
    validate_typed_data, Action as LegacyAction, Address, AmountSpec, ChainId, DexAction, DexFacts,
    DexTrace, Eip2612Action, Eip712Domain, Eip712OtherAction, Eip712TypedData, OracleRequirement,
    OracleRequirementKind, OtherAction, Permit2Action, Permit2Approval, Permit2PermitKind, Request,
    SignatureRequest, Token, TransactionRequest, TypedDataError, UsdValuation, WindowStatsContext,
};
pub use host::{
    Approvals, ApprovalsError, Clock, HostCapabilities, MockApprovals, MockClock, MockOracle,
    MockPortfolio, MockStatWindows, Oracle, OracleError, Portfolio, PortfolioError, ReservationId,
    StatDelta, StatKey, StatValue, StatWindows, SystemClock,
};
pub use lowering::{
    compute_dex_window_deltas, enrich_dex_action, enrich_dex_action_base, enrich_dex_window_stats,
    enrich_signature_action, request_from_action, request_from_action_with_host,
    requests_from_action, requests_from_actions,
};
pub use pipeline::{EvaluationOutcome, Pipeline, PipelineError, PipelineRequest};
pub use policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyError, PolicyRequest,
    PolicyRequestOrigin, Severity, Verdict,
};
pub use registry::{
    MockSignatureActionAdapterRegistry, MockTransactionActionAdapterRegistry,
    SignatureActionAdapterRegistry, SignatureActionResolverOutcome, TransactionActionAdapterIndex,
    TransactionActionAdapterRegistry, TransactionResolverOutcome,
};
pub use schema::PolicySchemaComposer;
