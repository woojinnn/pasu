//! `hl_update_leverage` reducer — upsert a per-asset leverage setting.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlUpdateLeverageAction;
use crate::error::ReducerResult;

// removed when task 6 wires the real (fallible) body
#[allow(clippy::unnecessary_wraps)]
pub(super) fn apply(
    _action: &HlUpdateLeverageAction,
    _state: &WalletState,
    _ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    Ok(StateDelta::new())
}
