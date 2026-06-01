//! Permission-domain walk + apply.
//!
//! Permission actions currently carry no live inputs, so both operations are
//! deterministic no-ops.

use serde_json::Value;

use simulation_reducer::action::permission::PermissionAction;
use simulation_state::Time;

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
