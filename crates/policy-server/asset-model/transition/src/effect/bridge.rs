//! `BridgeAction` reducers.
//!
//! A bridge moves a token out of the wallet on the source chain, but the
//! tracked wallet-state model does not simulate cross-chain balance flow, so the
//! reducer is a deterministic no-op; the structured `ActionBody` is what policy
//! review consumes (destination recipient/chain, output token/amount).

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::bridge::{BridgeAction, BridgeSendAction};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for BridgeAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Send(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for BridgeSendAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
