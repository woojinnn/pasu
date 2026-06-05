//! `GovernanceAction` reducers.
//!
//! Governance actions rotate voting/proposition power or operate on proposal
//! lifecycle state. Wallet balance/allowance state is unchanged, so reducers
//! are deterministic no-ops; the structured `ActionBody` is for policy review.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::governance::{
    GovernanceAction, GovernanceDelegateAction, GovernanceProposalRefAction,
    GovernanceProposeAction, GovernanceRedeemCancellationFeeAction,
    GovernanceUpdateRepresentativeAction, GovernanceVoteAction,
};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for GovernanceAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Delegate(a) => a.apply(state, ctx),
            Self::Vote(a) => a.apply(state, ctx),
            Self::Propose(a) => a.apply(state, ctx),
            Self::Cancel(a)
            | Self::ActivateVoting(a)
            | Self::Queue(a)
            | Self::Execute(a)
            | Self::StartVote(a)
            | Self::CloseVote(a) => a.apply(state, ctx),
            Self::RedeemCancellationFee(a) => a.apply(state, ctx),
            Self::UpdateRepresentative(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for GovernanceDelegateAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for GovernanceVoteAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for GovernanceProposeAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for GovernanceProposalRefAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for GovernanceRedeemCancellationFeeAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for GovernanceUpdateRepresentativeAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
