//! New 32-variant Action types (replacing the legacy 5-variant `core::LegacyAction`).
//! Mirrors `schema/schema/actions/` JSON definitions.

pub mod common;
pub mod envelope;
// Following modules added in subsequent tasks (1.3-1.6):
// pub mod dex;
// pub mod lending;
// pub mod misc;
// pub mod staking;
// pub mod restaking;

pub use common::{
    Address, AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString, Hex, UsdValuation,
    Validity, ValiditySource,
};
pub use envelope::{Action, ActionEnvelope, Category};
