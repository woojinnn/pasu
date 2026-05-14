//! Curated import surface for adapter authors and pipeline integrators.
//!
//! ```ignore
//! use policy_engine::prelude::*;
//! ```
//!
//! Re-exports the action envelope types, the surviving `core` domain types
//! consumed by host capabilities, the host capability trait surface, and the
//! `PolicyEngine` / `PolicyRequest` / `Verdict` evaluation surface.

pub use crate::action::{
    Action, ActionEnvelope, Address as ActionAddress, AmountConstraint, AmountKind, AssetKind,
    AssetRef, AssetRefWithAmountConstraint, Category, DecimalString, Hex,
    UsdValuation as ActionUsdValuation, Validity, ValiditySource,
};
pub use crate::core::{
    Address, AmountSpec, SignatureRequest, Token, TransactionRequest, UsdValuation,
};
pub use crate::host::{Approvals, ApprovalsError, MockApprovals};
pub use crate::host::{Clock, HostCapabilities, MockClock, Oracle, SystemClock};
pub use crate::host::{
    MockPortfolio, Portfolio, PortfolioError, StatDelta, StatKey, StatValue, StatWindows,
};
pub use crate::lowering::policy_request_from_envelope;
pub use crate::policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyError, PolicyRequest,
    PolicyRequestOrigin, Severity, Verdict,
};
pub use crate::root::{ProtocolRef, RequestKind, RootRequest};
