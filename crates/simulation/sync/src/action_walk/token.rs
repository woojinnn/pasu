//! Token 도메인 walk + apply.
//!
//! 현재 wired: (아직 없음). 후속에서 채움.
//! 대부분의 token 액션 (approve/transfer/...) 은 live_inputs 가 없거나 매우 적음
//! (permit nonce, permit2 nonce 정도).

use serde_json::Value;

use simulation_reducer::action::TokenAction;
use simulation_state::Time;

use crate::walker::{ActionSlot, StaleField, WalkStats};

pub(super) fn walk(
    _ta: &TokenAction,
    _action_index: usize,
    _now: Time,
    _stale: &mut Vec<StaleField>,
    _stats: &mut WalkStats,
) {
    // TODO: Erc20Permit.nonce, Permit2SignAllowance.nonce 만 LiveField
}

pub(super) fn apply(_ta: &mut TokenAction, _slot: &ActionSlot, _value: Value, _now: Time) {
    // TODO
}
