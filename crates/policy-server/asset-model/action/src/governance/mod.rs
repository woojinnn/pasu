//! `GovernanceAction` — DAO governance proposal, voting, and delegation.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ChainId, U256};
use policy_state::token::TokenRef;

pub mod delegate;
pub mod lifecycle;
pub mod propose;
pub mod vote;

pub use self::delegate::*;
pub use self::lifecycle::*;
pub use self::propose::*;
pub use self::vote::*;

/// User-level governance actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum GovernanceAction {
    /// Delegate voting/proposition power.
    Delegate(GovernanceDelegateAction),
    /// Vote on a proposal.
    Vote(GovernanceVoteAction),
    /// Create a proposal.
    Propose(GovernanceProposeAction),
    /// Cancel a proposal.
    Cancel(GovernanceProposalRefAction),
    /// Activate voting for a proposal.
    ActivateVoting(GovernanceProposalRefAction),
    /// Queue an approved proposal for execution.
    Queue(GovernanceProposalRefAction),
    /// Execute a queued proposal.
    Execute(GovernanceProposalRefAction),
    /// Start voting-machine vote collection for a proposal.
    StartVote(GovernanceProposalRefAction),
    /// Close voting-machine vote collection and send results.
    CloseVote(GovernanceProposalRefAction),
    /// Redeem a proposal cancellation fee.
    RedeemCancellationFee(GovernanceRedeemCancellationFeeAction),
    /// Update one governance representative.
    UpdateRepresentative(GovernanceUpdateRepresentativeAction),
}

impl GovernanceAction {
    /// The action's `serde` action tag.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Delegate(_) => "delegate",
            Self::Vote(_) => "vote",
            Self::Propose(_) => "propose",
            Self::Cancel(_) => "cancel",
            Self::ActivateVoting(_) => "activate_voting",
            Self::Queue(_) => "queue",
            Self::Execute(_) => "execute",
            Self::StartVote(_) => "start_vote",
            Self::CloseVote(_) => "close_vote",
            Self::RedeemCancellationFee(_) => "redeem_cancellation_fee",
            Self::UpdateRepresentative(_) => "update_representative",
        }
    }

    /// The venue name of the wrapped action.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::Delegate(a) => Some(a.venue.name()),
            Self::Vote(a) => Some(a.venue.name()),
            Self::Propose(a) => Some(a.venue.name()),
            Self::Cancel(a) | Self::ActivateVoting(a) => Some(a.venue.name()),
            Self::Queue(a) | Self::Execute(a) => Some(a.venue.name()),
            Self::StartVote(a) | Self::CloseVote(a) => Some(a.venue.name()),
            Self::RedeemCancellationFee(a) => Some(a.venue.name()),
            Self::UpdateRepresentative(a) => Some(a.venue.name()),
        }
    }
}

/// Governance venue identifier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum GovernanceVenue {
    /// Aave Governance V3 core contract.
    AaveGovernanceV3 {
        /// Chain hosting the deployment.
        chain: ChainId,
        /// Governance core contract.
        #[tsify(type = "string")]
        governance: Address,
    },
    /// Aave Governance V3 voting machine.
    AaveVotingMachine {
        /// Chain hosting the deployment.
        chain: ChainId,
        /// Voting machine contract.
        #[tsify(type = "string")]
        voting_machine: Address,
    },
    /// Governance-power token (AAVE, stkAAVE, aAAVE, etc.).
    GovernanceToken {
        /// Chain hosting the token.
        chain: ChainId,
        /// Token contract carrying voting/proposition power.
        token: TokenRef,
    },
}

impl GovernanceVenue {
    /// The venue's `serde` name tag.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::AaveGovernanceV3 { .. } => "aave_governance_v3",
            Self::AaveVotingMachine { .. } => "aave_voting_machine",
            Self::GovernanceToken { .. } => "governance_token",
        }
    }
}

/// Which governance power bucket is delegated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceDelegationKind {
    /// Generic token implementations where the call delegates all power.
    All,
    /// Aave/StakeToken voting power.
    Voting,
    /// Aave/StakeToken proposition power.
    Proposition,
    /// Unknown raw delegation type retained for forward compatibility.
    Raw,
}

/// Basic proposal reference shared by queue/execute.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GovernanceProposalRefAction {
    /// Governance venue.
    pub venue: GovernanceVenue,
    /// Proposal id.
    #[tsify(type = "string")]
    pub proposal_id: U256,
}
