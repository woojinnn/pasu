//! `LaunchpadAction` — `Commit`, `ClaimAllocation`, `ClaimVested`, `Refund`, `WithdrawCommit`. See spec §8.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::position::VestSchedule;
use simulation_state::primitives::{Time, U256};

pub mod claim_allocation;
pub mod claim_vested;
pub mod commit;
pub mod refund;
pub mod withdraw_commit;

pub use self::claim_allocation::*;
pub use self::claim_vested::*;
pub use self::commit::*;
pub use self::refund::*;
pub use self::withdraw_commit::*;

/// Launchpad-related actions covering subscription, claim, refund, and withdraw flows.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum LaunchpadAction {
    /// Commits funds to a launchpad sale (subscription).
    Commit(CommitAction),
    /// Claims the allocated sale tokens after the sale concludes.
    ClaimAllocation(ClaimAllocationAction),
    /// Claims tokens that have vested from a launchpad allocation.
    ClaimVested(ClaimVestedAction),
    /// Refunds the committed payment token (e.g. oversubscription or failed sale).
    Refund(RefundAction),
    /// Cancels a prior commitment on platforms that allow pre-sale withdrawal.
    WithdrawCommit(WithdrawCommitAction),
}

impl LaunchpadAction {
    /// The action's `serde` `action` tag (e.g. `"commit"`, `"claim_vested"`).
    ///
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Commit(_) => "commit",
            Self::ClaimAllocation(_) => "claim_allocation",
            Self::ClaimVested(_) => "claim_vested",
            Self::Refund(_) => "refund",
            Self::WithdrawCommit(_) => "withdraw_commit",
        }
    }

    /// Launchpad actions never carry a venue; always `None`.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        None
    }
}

/// Snapshot of a launchpad sale's on-chain state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SaleState {
    /// Whether the sale is currently open for commitments.
    pub is_active: bool,
    /// Total amount committed across all participants.
    #[tsify(type = "string")]
    pub total_committed: U256,
    /// Optional hard cap; commitments above this are rejected or refunded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub hard_cap: Option<U256>,
    /// Optional soft cap; sale may fail if not reached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub soft_cap: Option<U256>,
    /// Sale open/close timestamps as `(start, end)`.
    pub sale_window: (Time, Time),
    /// Optional `VestSchedule` applied to the claimed allocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub vest_schedule: Option<VestSchedule>,
}
