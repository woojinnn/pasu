//! `YieldAction` reducers.
//!
//! Yield-tokenization actions move tokens ↔ PT/YT/SY/LP or claim accrued yield.
//! The simulation state models generic token balances, but precise PT/YT
//! accounting + maturity exchange rates are deferred — each reducer is a
//! deterministic no-op (`StateDelta::new()`), mirroring `liquid_staking`. The
//! decode-path representation (`ActionBody`) and the Cedar policy layer are the
//! onboarding scope; balance/exchange-rate modeling is a later enrichment.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::yield_::{
    AddMarketLiquidityAction, CancelLimitOrderAction, ClaimYieldAction, MintPyAction, MintSyAction,
    PtSwapAction, RedeemPyAction, RedeemSyAction, RemoveMarketLiquidityAction,
    SignLimitOrderAction, YieldAction, YtSwapAction,
};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for YieldAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::PtSwap(a) => a.apply(state, ctx),
            Self::YtSwap(a) => a.apply(state, ctx),
            Self::AddMarketLiquidity(a) => a.apply(state, ctx),
            Self::RemoveMarketLiquidity(a) => a.apply(state, ctx),
            Self::MintPy(a) => a.apply(state, ctx),
            Self::RedeemPy(a) => a.apply(state, ctx),
            Self::MintSy(a) => a.apply(state, ctx),
            Self::RedeemSy(a) => a.apply(state, ctx),
            Self::ClaimYield(a) => a.apply(state, ctx),
            Self::SignLimitOrder(a) => a.apply(state, ctx),
            Self::CancelLimitOrder(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for PtSwapAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for YtSwapAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for AddMarketLiquidityAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for RemoveMarketLiquidityAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for MintPyAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for RedeemPyAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for MintSyAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for RedeemSyAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for ClaimYieldAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for SignLimitOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for CancelLimitOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
