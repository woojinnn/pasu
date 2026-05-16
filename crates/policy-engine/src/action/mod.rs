//! Normalized envelope-driven Action types.
//! Mirrors `schema/action-schema/schema/actions/` JSON definitions.

pub mod common;
/// Decentralized exchange action schema types.
pub mod dex;
pub mod envelope;
/// Lending action schema types.
pub mod lending;
/// Miscellaneous action schema types.
pub mod misc;
/// Restaking action schema types.
pub mod restaking;
/// Staking action schema types.
pub mod staking;

#[cfg(test)]
pub(crate) mod test_support;

pub use common::{
    Address, AmountConstraint, AmountKind, AssetKind, AssetRef, AssetRefWithAmountConstraint,
    DecimalString, Hex, Validity, ValiditySource,
};
pub use envelope::{Action, ActionEnvelope, Category};
