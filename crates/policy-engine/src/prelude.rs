//! Curated import surface for adapter authors and pipeline integrators.
//!
//! ```ignore
//! use policy_engine::prelude::*;
//! ```
//!
//! Re-exports the action envelope types, the surviving `core` domain types,
//! and the `PolicyEngine` / `PolicyRequest` / `Verdict` evaluation surface.

pub use crate::action::{
    Action, ActionEnvelope, Address as ActionAddress, AmountConstraint, AmountKind, AssetKind,
    AssetRef, AssetRefWithAmountConstraint, Category, DecimalString, Hex, Validity, ValiditySource,
};
pub use crate::core::{
    Address, AmountSpec, SignatureRequest, Token, TransactionRequest, UsdValuation,
};
pub use crate::lowering::{policy_request_from_envelope, LoweringError};
pub use crate::policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyError, PolicyRequest,
    PolicyRequestOrigin, Severity, Verdict,
};
pub use crate::root::{ProtocolRef, RequestKind, RootRequest};
