//! Permission-domain walk + apply.
//!
//! Permission actions currently carry no live inputs, so both operations are
//! deterministic no-ops.

use serde_json::Value;

use policy_state::Time;
use policy_transition::action::permission::PermissionAction;

use crate::walker::{ActionSlot, StaleField, WalkStats};

pub(super) fn walk(
    _action: &PermissionAction,
    _action_index: usize,
    _now: Time,
    _stale: &mut Vec<StaleField>,
    _stats: &mut WalkStats,
) {
}

pub(super) fn apply(_action: &mut PermissionAction, _slot: &ActionSlot, _value: Value, _now: Time) {
}
