//! AMM 도메인 walk + apply.
//!
//! 현재 wired: (아직 없음). 후속에서 채움.
//! 가장 슬롯 많은 도메인 (swap route, pool_state 등 복잡 타입).

use serde_json::Value;

use simulation_reducer::action::AmmAction;
use simulation_state::Time;

use crate::walker::{ActionSlot, StaleField, WalkStats};

pub(super) fn walk(
    _aa: &AmmAction,
    _action_index: usize,
    _now: Time,
    _stale: &mut Vec<StaleField>,
    _stats: &mut WalkStats,
) {
    // TODO: SwapAction (route/expected_out/price_impact_bp/gas_estimate),
    //       AddLiquidityAction, RemoveLiquidityAction, CollectFeesAction,
    //       SignIntentOrderAction
}

pub(super) fn apply(_aa: &mut AmmAction, _slot: &ActionSlot, _value: Value, _now: Time) {
    // TODO
}
