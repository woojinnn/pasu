//! `LiquidStakingAction` reducers.
//!
//! Liquid-staking actions move ETH ↔ staking tokens (stake / wrap / unwrap /
//! withdraw) or transfer shares. The simulation state models generic token
//! balances, but precise rebasing + withdrawal-queue accounting is deferred —
//! each reducer is a deterministic no-op (`StateDelta::new()`), mirroring
//! `lending::set_authorization`. The decode-path representation (`ActionBody`)
//! and the Cedar policy layer are the onboarding scope; balance/exchange-rate
//! modeling is a later enrichment.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::liquid_staking::{
    ClaimWithdrawalAction, LiquidStakingAction, RequestWithdrawalAction, StakeAction,
    TransferSharesAction, UnwrapAction, WrapAction,
};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for LiquidStakingAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Stake(a) => a.apply(state, ctx),
            Self::Wrap(a) => a.apply(state, ctx),
            Self::Unwrap(a) => a.apply(state, ctx),
            Self::RequestWithdrawal(a) => a.apply(state, ctx),
            Self::ClaimWithdrawal(a) => a.apply(state, ctx),
            Self::TransferShares(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for StakeAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for WrapAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for UnwrapAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for RequestWithdrawalAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for ClaimWithdrawalAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for TransferSharesAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
