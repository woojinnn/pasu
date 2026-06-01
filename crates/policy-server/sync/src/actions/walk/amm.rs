//! Wired: Swap (4), `AddLiquidity` (2), `RemoveLiquidity` (2), `CollectFees` (1),

use serde_json::Value;

use policy_state::Time;
use policy_transition::action::amm::{
    AddLiquidityAction, CollectFeesAction, RemoveLiquidityAction, SignIntentOrderAction, SwapAction,
};
use policy_transition::action::AmmAction;

use crate::walker::{ActionSlot, StaleField, WalkStats};

use super::{push_if_stale, set_field, value_to_decimal, value_to_u256};

pub(super) fn walk(
    aa: &AmmAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    match aa {
        AmmAction::Swap(s) => walk_swap(s, ix, now, st, sx),
        AmmAction::AddLiquidity(a) => walk_add(a, ix, now, st, sx),
        AmmAction::RemoveLiquidity(r) => walk_remove(r, ix, now, st, sx),
        AmmAction::CollectFees(c) => walk_collect(c, ix, now, st, sx),
        AmmAction::SignIntentOrder(s) => walk_sign_intent(s, ix, now, st, sx),
        AmmAction::CancelIntentOrder(_) => {}
    }
}

fn walk_swap(s: &SwapAction, ix: usize, now: Time, st: &mut Vec<StaleField>, sx: &mut WalkStats) {
    let li = &s.live_inputs;
    push_if_stale(st, sx, &li.route, now, ix, ActionSlot::AmmSwapRoute);
    push_if_stale(
        st,
        sx,
        &li.expected_amount_out,
        now,
        ix,
        ActionSlot::AmmSwapExpectedAmountOut,
    );
    push_if_stale(
        st,
        sx,
        &li.price_impact_bp,
        now,
        ix,
        ActionSlot::AmmSwapPriceImpactBp,
    );
    push_if_stale(
        st,
        sx,
        &li.gas_estimate,
        now,
        ix,
        ActionSlot::AmmSwapGasEstimate,
    );
}

fn walk_add(
    a: &AddLiquidityAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &a.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.pool_state,
        now,
        ix,
        ActionSlot::AmmAddLiquidityPoolState,
    );
    push_if_stale(
        st,
        sx,
        &li.current_price,
        now,
        ix,
        ActionSlot::AmmAddLiquidityCurrentPrice,
    );
}

fn walk_remove(
    r: &RemoveLiquidityAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &r.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.pool_state,
        now,
        ix,
        ActionSlot::AmmRemoveLiquidityPoolState,
    );
    push_if_stale(
        st,
        sx,
        &li.fees_owed,
        now,
        ix,
        ActionSlot::AmmRemoveLiquidityFeesOwed,
    );
}

fn walk_collect(
    c: &CollectFeesAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.fees_owed,
        now,
        ix,
        ActionSlot::AmmCollectFeesOwed,
    );
}

fn walk_sign_intent(
    s: &SignIntentOrderAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &s.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.expected_fill_price,
        now,
        ix,
        ActionSlot::AmmSignIntentExpectedFillPrice,
    );
    push_if_stale(
        st,
        sx,
        &li.competing_orders,
        now,
        ix,
        ActionSlot::AmmSignIntentCompetingOrders,
    );
}

pub(super) fn apply(aa: &mut AmmAction, slot: &ActionSlot, value: Value, now: Time) {
    match aa {
        AmmAction::Swap(s) => apply_swap(s, slot, value, now),
        AmmAction::AddLiquidity(a) => apply_add(a, slot, value, now),
        AmmAction::RemoveLiquidity(r) => apply_remove(r, slot, value, now),
        AmmAction::CollectFees(c) => apply_collect(c, slot, value, now),
        AmmAction::SignIntentOrder(s) => apply_sign_intent(s, slot, value, now),
        AmmAction::CancelIntentOrder(_) => {}
    }
}

fn apply_swap(s: &mut SwapAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut s.live_inputs;
    match slot {
        ActionSlot::AmmSwapRoute => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.route, v, now);
            }
        }
        ActionSlot::AmmSwapExpectedAmountOut => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.expected_amount_out, u, now);
            }
        }
        ActionSlot::AmmSwapPriceImpactBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.price_impact_bp, n as u32, now);
            }
        }
        ActionSlot::AmmSwapGasEstimate => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.gas_estimate, u, now);
            }
        }
        _ => {}
    }
}

fn apply_add(a: &mut AddLiquidityAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut a.live_inputs;
    match slot {
        ActionSlot::AmmAddLiquidityPoolState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.pool_state, v, now);
            }
        }
        ActionSlot::AmmAddLiquidityCurrentPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.current_price, d, now);
            }
        }
        _ => {}
    }
}

fn apply_remove(r: &mut RemoveLiquidityAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut r.live_inputs;
    match slot {
        ActionSlot::AmmRemoveLiquidityPoolState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.pool_state, v, now);
            }
        }
        ActionSlot::AmmRemoveLiquidityFeesOwed => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.fees_owed, v, now);
            }
        }
        _ => {}
    }
}

fn apply_collect(c: &mut CollectFeesAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    if matches!(slot, ActionSlot::AmmCollectFeesOwed) {
        if let Ok(v) = serde_json::from_value(value) {
            set_field(&mut li.fees_owed, v, now);
        }
    }
}

fn apply_sign_intent(s: &mut SignIntentOrderAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut s.live_inputs;
    match slot {
        ActionSlot::AmmSignIntentExpectedFillPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.expected_fill_price, d, now);
            }
        }
        ActionSlot::AmmSignIntentCompetingOrders => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.competing_orders, n as u32, now);
            }
        }
        _ => {}
    }
}
