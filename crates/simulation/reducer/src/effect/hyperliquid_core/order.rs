//! `hl_order` reducer — record an unfilled open-order intent.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlOrderAction;
use crate::error::ReducerResult;

// removed when task 5 wires the real (fallible) body
#[allow(clippy::unnecessary_wraps)]
pub(super) fn apply(
    _action: &HlOrderAction,
    _state: &WalletState,
    _ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    Ok(StateDelta::new())
}
