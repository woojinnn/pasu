//! New 32-variant Action types (replacing the legacy 5-variant `core::LegacyAction`).
//! Mirrors `schema/schema/actions/` JSON definitions.

pub mod common;
pub mod envelope;
// Following modules added in subsequent tasks (1.3-1.6):
/// Decentralized exchange action schema types.
pub mod dex;
/// Lending action schema types.
pub mod lending;
/// Miscellaneous action schema types.
pub mod misc;
/// Restaking action schema types.
pub mod restaking;
/// Staking action schema types.
pub mod staking;

pub use common::{
    Address, AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString, Hex, UsdValuation,
    Validity, ValiditySource,
};
pub use envelope::{Action, ActionEnvelope, Category};
