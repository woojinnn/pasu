//! `HyperliquidCoreAction` reducers — one file per action.
//!
//! Unlike the no-op this replaced, each HL action now records its effect as a
//! `StateDelta` against the wallet's single `HlAccount` position (id
//! [`common::HL_ACCOUNT_ID`]). No network fetch occurs: the reducer reads only
//! `state` + `ctx`. Absolute balances are obtained downstream by
//! `helpers::delta::apply_delta` layering the delta onto a Sync-populated base.

mod approve_agent;
mod common;
mod order;
mod update_leverage;
mod usd_send;
mod withdraw;

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HyperliquidCoreAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for HyperliquidCoreAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Order(a) => order::apply(a, state, ctx),
            Self::UpdateLeverage(a) => update_leverage::apply(a, state, ctx),
            Self::Withdraw(a) => withdraw::apply(a, state, ctx),
            Self::UsdSend(a) => usd_send::apply(a, state, ctx),
            Self::ApproveAgent(a) => approve_agent::apply(a, state, ctx),
        }
    }
}
