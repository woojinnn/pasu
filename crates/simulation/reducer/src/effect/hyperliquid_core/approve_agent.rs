//! `hl_approve_agent` reducer — record a delegated agent wallet.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlApproveAgentAction;
use crate::error::ReducerResult;

// removed when task 9 wires the real (fallible) body
#[allow(clippy::unnecessary_wraps)]
pub(super) fn apply(
    _action: &HlApproveAgentAction,
    _state: &WalletState,
    _ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    Ok(StateDelta::new())
}
