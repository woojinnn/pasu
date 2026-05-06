//! Curated import surface for **adapter authors**.
//!
//! ```ignore
//! use policy_engine::prelude::*;
//! ```
//!
//! This module re-exports the trait surface and supporting types an
//! adapter implementation typically needs: the `Adapter` trait, `AdapterId`,
//! `AdapterError`, `MatchKey`, the domain types (`Action`, `Token`,
//! `TransactionRequest`, `AmountSpec`, `UsdValuation`, `SwapAction`), and
//! the `Oracle` trait + `PolicyRequest` (used in `Adapter::into_request`).
//!
//! `alloy_primitives` and `alloy_sol_types` are *not* re-exported. The
//! `sol!` macro hard-codes its expanded code's paths to `::alloy_sol_types`,
//! so adapter crates must depend on those crates directly anyway; an
//! intermediate re-export would only mislead callers.

pub use crate::adapter::{
    ActionKind, Adapter, AdapterDescriptor, AdapterError, AdapterFactory, AdapterId, AdapterKind,
    ContractTarget, MatchKey, SolidityFunction, SolidityFunctionSpec, StaticAdapterFactory,
    TypedAdapter,
};
pub use crate::approvals::{Approvals, ApprovalsError, MockApprovals};
pub use crate::core::{
    Action, Address, AmountSpec, ChainId, MultiAction, SwapAction, Token, TransactionRequest,
    UsdValuation,
};
pub use crate::lowering::{enrich_actions_with_usd, requests_from_actions};
pub use crate::portfolio::{MockPortfolio, Portfolio, PortfolioError};
pub use crate::host::HostCapabilities;
pub use crate::oracle::Oracle;
pub use crate::policy::PolicyRequest;
