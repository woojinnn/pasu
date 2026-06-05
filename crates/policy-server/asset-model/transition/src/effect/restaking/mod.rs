//! `RestakingAction` reducers.
//!
//! Restaking actions move shares between operators (delegate / undelegate),
//! into strategies (deposit), and through the withdrawal queue. The simulation
//! state models generic token balances, but precise restaking-share +
//! withdrawal-queue accounting is deferred — each reducer is a deterministic
//! no-op (`StateDelta::new()`), mirroring `liquid_staking`. The decode-path
//! representation (`ActionBody`) and the Cedar policy layer are the onboarding
//! scope; balance/share modeling is a later enrichment.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::restaking::{
    CompleteWithdrawalAction, DelegateToAction, DepositAction, QueueWithdrawalAction,
    RedelegateAction, RegisterOperatorAction, RestakingAction, UndelegateAction,
};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for RestakingAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::DelegateTo(a) => a.apply(state, ctx),
            Self::Redelegate(a) => a.apply(state, ctx),
            Self::Undelegate(a) => a.apply(state, ctx),
            Self::Deposit(a) => a.apply(state, ctx),
            Self::QueueWithdrawal(a) => a.apply(state, ctx),
            Self::CompleteWithdrawal(a) => a.apply(state, ctx),
            Self::RegisterOperator(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for DelegateToAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for RedelegateAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for UndelegateAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for DepositAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for QueueWithdrawalAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for CompleteWithdrawalAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for RegisterOperatorAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
