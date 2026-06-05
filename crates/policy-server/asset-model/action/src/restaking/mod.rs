//! `RestakingAction` — `EigenLayer` restaking: operator delegation, strategy
//! share deposits, the queued-withdrawal lifecycle, and operator registration.
//!
//! New domain (extension-guide axis 1). Mirrors the `liquid_staking` layout: a
//! venue enum (`RestakingVenue`) + per-action structs + `action_tag()` /
//! `venue_name()`. Round-1 actions are faithful static decodes (no live
//! inputs); the abstract `deposit_shares` unit on `queue_withdrawal` is
//! enrichment-deferred (per-strategy share→underlying view, array-shaped).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::ChainId;

pub mod complete_withdrawal;
pub mod delegate_to;
pub mod deposit;
pub mod queue_withdrawal;
pub mod redelegate;
pub mod register_operator;
pub mod undelegate;

pub use self::complete_withdrawal::*;
pub use self::delegate_to::*;
pub use self::deposit::*;
pub use self::queue_withdrawal::*;
pub use self::redelegate::*;
pub use self::register_operator::*;
pub use self::undelegate::*;

/// User-level restaking actions across supported venues (currently `EigenLayer`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RestakingAction {
    /// Delegate all deposited restaking shares to an operator (`delegateTo`).
    DelegateTo(DelegateToAction),
    /// Atomically undelegate from the current operator and delegate to a new
    /// one (`redelegate`).
    Redelegate(RedelegateAction),
    /// Undelegate from the current operator — queues a withdrawal of all shares.
    Undelegate(UndelegateAction),
    /// Deposit an ERC-20 (LST) into a strategy, minting restaking shares
    /// (`depositIntoStrategy` / off-chain `Deposit`).
    Deposit(DepositAction),
    /// Queue a withdrawal of staked shares (`queueWithdrawals`).
    QueueWithdrawal(QueueWithdrawalAction),
    /// Complete a queued withdrawal, releasing funds (`completeQueuedWithdrawal(s)`).
    CompleteWithdrawal(CompleteWithdrawalAction),
    /// Register the caller as an `EigenLayer` operator (`registerAsOperator`).
    RegisterOperator(RegisterOperatorAction),
}

impl RestakingAction {
    /// The action's `serde` `action` tag (e.g. `"delegate_to"`).
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::DelegateTo(_) => "delegate_to",
            Self::Redelegate(_) => "redelegate",
            Self::Undelegate(_) => "undelegate",
            Self::Deposit(_) => "deposit",
            Self::QueueWithdrawal(_) => "queue_withdrawal",
            Self::CompleteWithdrawal(_) => "complete_withdrawal",
            Self::RegisterOperator(_) => "register_operator",
        }
    }

    /// The venue `name` of the wrapped action. Every restaking action carries a venue.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::DelegateTo(a) => Some(a.venue.name()),
            Self::Redelegate(a) => Some(a.venue.name()),
            Self::Undelegate(a) => Some(a.venue.name()),
            Self::Deposit(a) => Some(a.venue.name()),
            Self::QueueWithdrawal(a) => Some(a.venue.name()),
            Self::CompleteWithdrawal(a) => Some(a.venue.name()),
            Self::RegisterOperator(a) => Some(a.venue.name()),
        }
    }
}

// ---------------------------------------------------------------------------
// Venue
// ---------------------------------------------------------------------------

/// Restaking venue identifier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum RestakingVenue {
    /// `EigenLayer` deployment on a given chain (`DelegationManager` /
    /// `StrategyManager` / `EigenPodManager` set).
    #[serde(rename = "eigenlayer")]
    EigenLayer {
        /// Chain hosting the `EigenLayer` deployment.
        chain: ChainId,
    },
}

impl RestakingVenue {
    /// The venue's `serde` `name` tag (`"eigenlayer"`).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EigenLayer { .. } => "eigenlayer",
        }
    }
}
