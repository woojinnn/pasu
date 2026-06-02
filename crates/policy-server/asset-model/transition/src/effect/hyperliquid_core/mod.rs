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

use policy_state::{EvalContext, StateDelta, WalletState};

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
            // StateDelta modeling for these fund-movement / transfer actions is
            // deferred to the simulation track (it needs the spot-balance / vault
            // / staking position model). The VERDICT path (lowering_v2 → Cedar)
            // does not call `Reducer::apply`, so a no-op delta here has zero
            // effect on policy decisions; it only under-reports the simulated
            // balance change, which the simulation track will fill in.
            Self::SpotSend(_)
            | Self::UsdClassTransfer(_)
            | Self::SendAsset(_)
            | Self::SendToEvmWithData(_)
            | Self::CDeposit(_)
            | Self::CWithdraw(_)
            | Self::VaultTransfer(_)
            | Self::SubAccountTransfer(_)
            | Self::ApproveBuilderFee(_)
            | Self::TokenDelegate(_)
            | Self::TwapOrder(_)
            | Self::UpdateIsolatedMargin(_)
            | Self::Unknown(_) => Ok(StateDelta::new()),
        }
    }
}
