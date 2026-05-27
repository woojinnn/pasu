//! Normalized envelope-driven Action types.
//! Mirrors `schema/action-schema/schema/actions/` JSON definitions.
//!
//! ## v3 bridge
//!
//! Phase 2H exposes the new hierarchical `ActionBody` tree from
//! `simulation-reducer` under [`v3`]. Downstream consumers (registryV2 mapper,
//! SW wire, Cedar entry) migrate to `v3` gradually; the legacy flat
//! [`envelope::Action`] / [`envelope::Category`] enum remains the source of
//! truth for the existing pipeline until Phase 5 cutover.

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

/// PDF FSM spec `ActionBody` tree — re-exported from `simulation-reducer`.
/// Bridges the legacy flat `action::envelope::Action` (above) and the new
/// hierarchical reducer-side types. Phase 3+ consumers should depend on this
/// namespace.
pub use simulation_reducer::action as v3;
