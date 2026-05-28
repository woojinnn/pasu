//! Perp 도메인 walk + apply.
//!
//! 현재 wired: (아직 없음). 후속에서 채움.
//! 가장 슬롯 많은 도메인 — Open(10) + Close/Increase/Decrease + 주문/취소 등.

use serde_json::Value;

use simulation_reducer::action::PerpAction;
use simulation_state::Time;

use crate::walker::{ActionSlot, StaleField, WalkStats};

pub(super) fn walk(
    _pa: &PerpAction,
    _action_index: usize,
    _now: Time,
    _stale: &mut Vec<StaleField>,
    _stats: &mut WalkStats,
) {
    // TODO: OpenPerpAction (mark/oracle/funding/available_oi/max_leverage/
    //       initial_margin/maintenance/fee_taker/fee_maker/user_account_state),
    //       Close, Increase, Decrease, AdjustMargin, ChangeLeverage,
    //       ChangeMarginMode, PlaceLimit, PlaceStop, ClaimFunding
}

pub(super) fn apply(_pa: &mut PerpAction, _slot: &ActionSlot, _value: Value, _now: Time) {
    // TODO
}
