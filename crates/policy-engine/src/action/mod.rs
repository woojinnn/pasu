//! Shared action scalar types and the v3 `ActionBody` bridge.
//!
//! ## v3 bridge
//!
//! Phase 2H exposes the new hierarchical `ActionBody` tree from
//! `simulation-reducer` under [`v3`]. This is the action model the v2/v3
//! verdict pipeline depends on. The legacy flat action model was removed in the
//! Phase 1 action restructure; only the shared scalar newtypes in [`common`]
//! survive (they are consumed by `abi-resolver` and the v3 decode path).

pub mod common;

pub use common::{
    Address, AmountConstraint, AmountKind, AssetKind, AssetRef, AssetRefWithAmountConstraint,
    DecimalString, Hex, Validity, ValiditySource,
};

/// PDF FSM spec `ActionBody` tree — re-exported from `simulation-reducer`.
/// This is the hierarchical reducer-side action model. Phase 3+ consumers
/// depend on this namespace.
pub use simulation_reducer::action as v3;
