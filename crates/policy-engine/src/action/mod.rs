//! Shared action scalar types and the v3 `ActionBody` bridge.
//!
//! ## v3 bridge
//!
//! `policy-transition` under [`v3`]. This is the action model the v2/v3
//! verdict pipeline depends on. The legacy flat envelope model
//! (`Action`/`ActionEnvelope`/`Category`) was removed when the hierarchical
//! model became canonical. Only the shared scalar newtypes in [`common`] survive
//! for `abi-resolver` and the v3 decode path.

pub mod common;

pub use common::{
    Address, AmountConstraint, AmountKind, AssetKind, AssetRef, AssetRefWithAmountConstraint,
    DecimalString, Hex, Validity, ValiditySource,
};

/// PDF FSM spec `ActionBody` tree — re-exported from `policy-transition`.
/// This is the hierarchical reducer-side action model. Phase 3+ consumers
/// depend on this namespace.
pub use policy_transition::action as v3;
