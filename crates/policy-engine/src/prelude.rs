//! Curated import surface for **adapter authors**.
//!
//! ```ignore
//! use policy_engine::prelude::*;
//! ```
//!
//! This module re-exports the trait surface and supporting types an
//! adapter implementation typically needs: the `Adapter` trait, `AdapterId`,
//! `AdapterError`, `MatchKey`, the domain types (`Action`, `Token`,
//! `TransactionRequest`, `AmountSpec`, `UsdValuation`, `DexAction`), and
//! the `Oracle` trait + `PolicyRequest` (used by the policy evaluator surface).
//!
//! `alloy_primitives` and `alloy_sol_types` are *not* re-exported. The
//! `sol!` macro hard-codes its expanded code's paths to `::alloy_sol_types`,
//! so adapter crates must depend on those crates directly anyway; an
//! intermediate re-export would only mislead callers.

pub use crate::adapter::{
    ActionKind, Adapter, AdapterDescriptor, AdapterError, AdapterFactory, AdapterId, AdapterKind,
    ContractTarget, MatchKey, SignatureAdapter, SignatureMatchKey, SolidityFunction,
    SolidityFunctionSpec, StaticAdapterFactory, TypedAdapter,
};
pub use crate::core::{
    validate_typed_data, Action, Address, AmountSpec, ChainId, DexAction, DexFacts, DexTrace,
    Eip2612Action, Eip712Domain, Eip712OtherAction, Eip712TypedData, OracleRequirement,
    OracleRequirementKind, OtherAction, Permit2Action, Permit2Approval, Permit2PermitKind, Request,
    SignatureRequest, Token, TransactionRequest, TypedDataError, UsdValuation, WindowStatsContext,
};
pub use crate::host::{Approvals, ApprovalsError, MockApprovals};
pub use crate::host::{Clock, HostCapabilities, MockClock, Oracle, SystemClock};
pub use crate::host::{
    MockPortfolio, Portfolio, PortfolioError, StatDelta, StatKey, StatValue, StatWindows,
};
pub use crate::lowering::{
    compute_dex_window_deltas, enrich_dex_action, enrich_dex_action_base, enrich_dex_window_stats,
    enrich_signature_action, request_from_action_with_host, requests_from_action,
    requests_from_actions,
};
pub use crate::policy::PolicyRequest;
pub use crate::registry::{MockSignatureRegistry, SignatureRegistry, SignatureResolverOutcome};
