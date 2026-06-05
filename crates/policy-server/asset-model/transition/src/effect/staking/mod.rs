//! `StakingAction` reducers.
//!
//! Staking actions lock/unlock CRV for vote-escrow, mint reward emissions, or
//! reallocate gauge vote weight. The simulation state models generic token
//! balances, but precise vote-escrow + emission accounting is deferred — each
//! reducer is a deterministic no-op (`StateDelta::new()`), mirroring
//! `liquid_staking` and `lending::set_authorization`. The decode-path
//! representation (`ActionBody`) and the Cedar policy layer are the onboarding
//! scope; balance/boost/lock-state modeling is a later enrichment.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::staking::{
    ClaimRewardsAction, CooldownAction, GaugeDepositAction, GaugeWithdrawAction,
    IncreaseLockAmountAction, IncreaseLockTimeAction, LockAction, RedeemAction, StakeAction,
    StakingAction, UnlockAction, VoteForGaugeAction,
};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for StakingAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Lock(a) => a.apply(state, ctx),
            Self::IncreaseLockAmount(a) => a.apply(state, ctx),
            Self::IncreaseLockTime(a) => a.apply(state, ctx),
            Self::Unlock(a) => a.apply(state, ctx),
            Self::ClaimRewards(a) => a.apply(state, ctx),
            Self::VoteForGauge(a) => a.apply(state, ctx),
            Self::GaugeDeposit(a) => a.apply(state, ctx),
            Self::GaugeWithdraw(a) => a.apply(state, ctx),
            Self::Stake(a) => a.apply(state, ctx),
            Self::Cooldown(a) => a.apply(state, ctx),
            Self::Redeem(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for LockAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for IncreaseLockAmountAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for IncreaseLockTimeAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for UnlockAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for ClaimRewardsAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for VoteForGaugeAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for GaugeDepositAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for GaugeWithdrawAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for StakeAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for CooldownAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for RedeemAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
