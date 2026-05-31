//! `hl_usd_send` reducer — record a USDC transfer.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlUsdSendAction;
use crate::error::ReducerResult;

// removed when task 8 wires the real (fallible) body
#[allow(clippy::unnecessary_wraps)]
pub(super) fn apply(
    _action: &HlUsdSendAction,
    _state: &WalletState,
    _ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    Ok(StateDelta::new())
}
