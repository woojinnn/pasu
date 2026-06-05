//! `AirdropAction` — `Claim`, `Delegate`. See spec §7.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// `ClaimAirdropAction` and related claim-target / live-input types.
pub mod claim;
/// `DelegateGovernanceAction` and related live-input types.
pub mod delegate;

pub use self::claim::*;
pub use self::delegate::*;

/// Airdrop-related actions: claiming a one-time distribution or delegating governance voting power.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AirdropAction {
    /// Claim eligibility for a one-time airdrop (Merkle, signature, or staking-based).
    Claim(ClaimAirdropAction),
    /// Delegate governance voting power for a governance token (e.g. UNI, COMP, ENS).
    Delegate(DelegateGovernanceAction),
}

impl AirdropAction {
    /// The action's `serde` `action` tag (`"claim"` or `"delegate"`).
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Claim(_) => "claim",
            Self::Delegate(_) => "delegate",
        }
    }

    /// Airdrop actions never carry a venue; always `None`.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        None
    }
}
