//! `hl_withdraw` reducer — record a USDC withdrawal.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlWithdrawAction;
use crate::error::ReducerResult;

// removed when task 7 wires the real (fallible) body
#[allow(clippy::unnecessary_wraps)]
pub(super) fn apply(
    _action: &HlWithdrawAction,
    _state: &WalletState,
    _ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    Ok(StateDelta::new())
}
