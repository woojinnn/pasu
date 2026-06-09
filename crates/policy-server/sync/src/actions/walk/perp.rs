//! Wired: Open/Close/Increase/Decrease (10+4+10+4), `AdjustMargin` (2),
//!        `ChangeLeverage` (3), `ChangeMarginMode` (2), `PlaceLimit` (4),

use serde_json::Value;

use policy_state::{SignedI256, Time};
use policy_transition::action::perp::{
    AdjustMarginAction, ChangeLeverageAction, ChangeMarginModeAction, ClaimFundingAction,
    ClosePerpAction, DecreasePerpAction, IncreasePerpAction, OpenPerpAction, PlaceOrderAction,
};
use policy_transition::action::PerpAction;

use crate::walker::{ActionSlot, StaleField, WalkStats};

use super::{push_if_stale, set_field, value_to_decimal, value_to_u256};

pub(super) fn walk(
    pa: &PerpAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    match pa {
        PerpAction::OpenPosition(o) => walk_open(o, ix, now, st, sx),
        PerpAction::ClosePosition(c) => walk_close(c, ix, now, st, sx),
        PerpAction::IncreasePosition(i) => walk_increase(i, ix, now, st, sx),
        PerpAction::DecreasePosition(d) => walk_decrease(d, ix, now, st, sx),
        PerpAction::AdjustMargin(a) => walk_adjust(a, ix, now, st, sx),
        PerpAction::ChangeLeverage(c) => walk_change_lev(c, ix, now, st, sx),
        PerpAction::ChangeMarginMode(c) => walk_change_mm(c, ix, now, st, sx),
        PerpAction::PlaceOrder(p) => walk_place_order(p, ix, now, st, sx),
        PerpAction::CancelOrder(_) => {}
        PerpAction::ClaimFunding(c) => walk_claim_funding(c, ix, now, st, sx),
    }
}

fn walk_open(
    o: &OpenPerpAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &o.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.mark_price,
        now,
        ix,
        ActionSlot::PerpOpenMarkPrice,
    );
    push_if_stale(
        st,
        sx,
        &li.oracle_price,
        now,
        ix,
        ActionSlot::PerpOpenOraclePrice,
    );
    push_if_stale(
        st,
        sx,
        &li.funding_rate,
        now,
        ix,
        ActionSlot::PerpOpenFundingRate,
    );
    push_if_stale(
        st,
        sx,
        &li.available_oi,
        now,
        ix,
        ActionSlot::PerpOpenAvailableOi,
    );
    push_if_stale(
        st,
        sx,
        &li.max_leverage,
        now,
        ix,
        ActionSlot::PerpOpenMaxLeverage,
    );
    push_if_stale(
        st,
        sx,
        &li.initial_margin_bp,
        now,
        ix,
        ActionSlot::PerpOpenInitialMarginBp,
    );
    push_if_stale(
        st,
        sx,
        &li.maintenance_bp,
        now,
        ix,
        ActionSlot::PerpOpenMaintenanceBp,
    );
    push_if_stale(
        st,
        sx,
        &li.fee_taker_bp,
        now,
        ix,
        ActionSlot::PerpOpenFeeTakerBp,
    );
    push_if_stale(
        st,
        sx,
        &li.fee_maker_bp,
        now,
        ix,
        ActionSlot::PerpOpenFeeMakerBp,
    );
    push_if_stale(
        st,
        sx,
        &li.user_account_state,
        now,
        ix,
        ActionSlot::PerpOpenUserAccountState,
    );
}

fn walk_close(
    c: &ClosePerpAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.mark_price,
        now,
        ix,
        ActionSlot::PerpCloseMarkPrice,
    );
    push_if_stale(
        st,
        sx,
        &li.unrealized_pnl_now,
        now,
        ix,
        ActionSlot::PerpCloseUnrealizedPnl,
    );
    push_if_stale(
        st,
        sx,
        &li.funding_accrued,
        now,
        ix,
        ActionSlot::PerpCloseFundingAccrued,
    );
    push_if_stale(st, sx, &li.fee_bp, now, ix, ActionSlot::PerpCloseFeeBp);
}

fn walk_increase(
    i: &IncreasePerpAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &i.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.mark_price,
        now,
        ix,
        ActionSlot::PerpIncreaseMarkPrice,
    );
    push_if_stale(
        st,
        sx,
        &li.oracle_price,
        now,
        ix,
        ActionSlot::PerpIncreaseOraclePrice,
    );
    push_if_stale(
        st,
        sx,
        &li.funding_rate,
        now,
        ix,
        ActionSlot::PerpIncreaseFundingRate,
    );
    push_if_stale(
        st,
        sx,
        &li.available_oi,
        now,
        ix,
        ActionSlot::PerpIncreaseAvailableOi,
    );
    push_if_stale(
        st,
        sx,
        &li.max_leverage,
        now,
        ix,
        ActionSlot::PerpIncreaseMaxLeverage,
    );
    push_if_stale(
        st,
        sx,
        &li.initial_margin_bp,
        now,
        ix,
        ActionSlot::PerpIncreaseInitialMarginBp,
    );
    push_if_stale(
        st,
        sx,
        &li.maintenance_bp,
        now,
        ix,
        ActionSlot::PerpIncreaseMaintenanceBp,
    );
    push_if_stale(
        st,
        sx,
        &li.fee_taker_bp,
        now,
        ix,
        ActionSlot::PerpIncreaseFeeTakerBp,
    );
    push_if_stale(
        st,
        sx,
        &li.fee_maker_bp,
        now,
        ix,
        ActionSlot::PerpIncreaseFeeMakerBp,
    );
    push_if_stale(
        st,
        sx,
        &li.user_account_state,
        now,
        ix,
        ActionSlot::PerpIncreaseUserAccountState,
    );
}

fn walk_decrease(
    d: &DecreasePerpAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &d.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.mark_price,
        now,
        ix,
        ActionSlot::PerpDecreaseMarkPrice,
    );
    push_if_stale(
        st,
        sx,
        &li.unrealized_pnl_now,
        now,
        ix,
        ActionSlot::PerpDecreaseUnrealizedPnl,
    );
    push_if_stale(
        st,
        sx,
        &li.funding_accrued,
        now,
        ix,
        ActionSlot::PerpDecreaseFundingAccrued,
    );
    push_if_stale(st, sx, &li.fee_bp, now, ix, ActionSlot::PerpDecreaseFeeBp);
}

fn walk_adjust(
    a: &AdjustMarginAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &a.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.position_state,
        now,
        ix,
        ActionSlot::PerpAdjustMarginPositionState,
    );
    push_if_stale(
        st,
        sx,
        &li.free_margin_after,
        now,
        ix,
        ActionSlot::PerpAdjustMarginFreeMarginAfter,
    );
}

fn walk_change_lev(
    c: &ChangeLeverageAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.max_leverage,
        now,
        ix,
        ActionSlot::PerpChangeLeverageMaxLeverage,
    );
    push_if_stale(
        st,
        sx,
        &li.affected_positions,
        now,
        ix,
        ActionSlot::PerpChangeLeverageAffectedPositions,
    );
    push_if_stale(
        st,
        sx,
        &li.new_liq_prices,
        now,
        ix,
        ActionSlot::PerpChangeLeverageNewLiqPrices,
    );
}

fn walk_change_mm(
    c: &ChangeMarginModeAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.affected_positions,
        now,
        ix,
        ActionSlot::PerpChangeMarginModeAffectedPositions,
    );
    push_if_stale(
        st,
        sx,
        &li.margin_reallocation,
        now,
        ix,
        ActionSlot::PerpChangeMarginModeReallocation,
    );
}

fn walk_place_order(
    p: &PlaceOrderAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    // Hyperliquid pre-sign orders carry no live inputs — nothing to refresh.
    let Some(li) = &p.live_inputs else {
        return;
    };
    push_if_stale(
        st,
        sx,
        &li.mark_price,
        now,
        ix,
        ActionSlot::PerpPlaceLimitMarkPrice,
    );
    push_if_stale(
        st,
        sx,
        &li.best_bid_ask,
        now,
        ix,
        ActionSlot::PerpPlaceLimitBestBidAsk,
    );
    push_if_stale(
        st,
        sx,
        &li.open_orders_count,
        now,
        ix,
        ActionSlot::PerpPlaceLimitOpenOrdersCount,
    );
    push_if_stale(
        st,
        sx,
        &li.user_account_state,
        now,
        ix,
        ActionSlot::PerpPlaceLimitUserAccountState,
    );
}

fn walk_claim_funding(
    c: &ClaimFundingAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &c.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.claimable,
        now,
        ix,
        ActionSlot::PerpClaimFundingClaimable,
    );
}

// ─────────────────────── apply ───────────────────────

pub(super) fn apply(pa: &mut PerpAction, slot: &ActionSlot, value: Value, now: Time) {
    match pa {
        PerpAction::OpenPosition(o) => apply_open(o, slot, value, now),
        PerpAction::ClosePosition(c) => apply_close(c, slot, value, now),
        PerpAction::IncreasePosition(i) => apply_increase(i, slot, value, now),
        PerpAction::DecreasePosition(d) => apply_decrease(d, slot, value, now),
        PerpAction::AdjustMargin(a) => apply_adjust(a, slot, value, now),
        PerpAction::ChangeLeverage(c) => apply_change_lev(c, slot, value, now),
        PerpAction::ChangeMarginMode(c) => apply_change_mm(c, slot, value, now),
        PerpAction::PlaceOrder(p) => apply_place_order(p, slot, value, now),
        PerpAction::CancelOrder(_) => {}
        PerpAction::ClaimFunding(c) => apply_claim_funding(c, slot, value, now),
    }
}

fn apply_open(o: &mut OpenPerpAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut o.live_inputs;
    match slot {
        ActionSlot::PerpOpenMarkPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.mark_price, d, now);
            }
        }
        ActionSlot::PerpOpenOraclePrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.oracle_price, d, now);
            }
        }
        ActionSlot::PerpOpenFundingRate => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.funding_rate, d, now);
            }
        }
        ActionSlot::PerpOpenAvailableOi => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.available_oi, u, now);
            }
        }
        ActionSlot::PerpOpenMaxLeverage => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.max_leverage, d, now);
            }
        }
        ActionSlot::PerpOpenInitialMarginBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.initial_margin_bp, n as u32, now);
            }
        }
        ActionSlot::PerpOpenMaintenanceBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.maintenance_bp, n as u32, now);
            }
        }
        ActionSlot::PerpOpenFeeTakerBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.fee_taker_bp, n as u32, now);
            }
        }
        ActionSlot::PerpOpenFeeMakerBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.fee_maker_bp, n as u32, now);
            }
        }
        ActionSlot::PerpOpenUserAccountState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_account_state, v, now);
            }
        }
        _ => {}
    }
}

fn apply_close(c: &mut ClosePerpAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    match slot {
        ActionSlot::PerpCloseMarkPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.mark_price, d, now);
            }
        }
        ActionSlot::PerpCloseUnrealizedPnl => {
            if let Some(s) = value_to_i256(&value) {
                set_field(&mut li.unrealized_pnl_now, s, now);
            }
        }
        ActionSlot::PerpCloseFundingAccrued => {
            if let Some(s) = value_to_i256(&value) {
                set_field(&mut li.funding_accrued, s, now);
            }
        }
        ActionSlot::PerpCloseFeeBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.fee_bp, n as u32, now);
            }
        }
        _ => {}
    }
}

fn apply_increase(i: &mut IncreasePerpAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut i.live_inputs;
    match slot {
        ActionSlot::PerpIncreaseMarkPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.mark_price, d, now);
            }
        }
        ActionSlot::PerpIncreaseOraclePrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.oracle_price, d, now);
            }
        }
        ActionSlot::PerpIncreaseFundingRate => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.funding_rate, d, now);
            }
        }
        ActionSlot::PerpIncreaseAvailableOi => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.available_oi, u, now);
            }
        }
        ActionSlot::PerpIncreaseMaxLeverage => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.max_leverage, d, now);
            }
        }
        ActionSlot::PerpIncreaseInitialMarginBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.initial_margin_bp, n as u32, now);
            }
        }
        ActionSlot::PerpIncreaseMaintenanceBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.maintenance_bp, n as u32, now);
            }
        }
        ActionSlot::PerpIncreaseFeeTakerBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.fee_taker_bp, n as u32, now);
            }
        }
        ActionSlot::PerpIncreaseFeeMakerBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.fee_maker_bp, n as u32, now);
            }
        }
        ActionSlot::PerpIncreaseUserAccountState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_account_state, v, now);
            }
        }
        _ => {}
    }
}

fn apply_decrease(d: &mut DecreasePerpAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut d.live_inputs;
    match slot {
        ActionSlot::PerpDecreaseMarkPrice => {
            if let Some(d2) = value_to_decimal(&value) {
                set_field(&mut li.mark_price, d2, now);
            }
        }
        ActionSlot::PerpDecreaseUnrealizedPnl => {
            if let Some(s) = value_to_i256(&value) {
                set_field(&mut li.unrealized_pnl_now, s, now);
            }
        }
        ActionSlot::PerpDecreaseFundingAccrued => {
            if let Some(s) = value_to_i256(&value) {
                set_field(&mut li.funding_accrued, s, now);
            }
        }
        ActionSlot::PerpDecreaseFeeBp => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.fee_bp, n as u32, now);
            }
        }
        _ => {}
    }
}

fn apply_adjust(a: &mut AdjustMarginAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut a.live_inputs;
    match slot {
        ActionSlot::PerpAdjustMarginPositionState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.position_state, v, now);
            }
        }
        ActionSlot::PerpAdjustMarginFreeMarginAfter => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.free_margin_after, u, now);
            }
        }
        _ => {}
    }
}

fn apply_change_lev(c: &mut ChangeLeverageAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    match slot {
        ActionSlot::PerpChangeLeverageMaxLeverage => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.max_leverage, d, now);
            }
        }
        ActionSlot::PerpChangeLeverageAffectedPositions => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.affected_positions, v, now);
            }
        }
        ActionSlot::PerpChangeLeverageNewLiqPrices => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.new_liq_prices, v, now);
            }
        }
        _ => {}
    }
}

fn apply_change_mm(c: &mut ChangeMarginModeAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    match slot {
        ActionSlot::PerpChangeMarginModeAffectedPositions => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.affected_positions, v, now);
            }
        }
        ActionSlot::PerpChangeMarginModeReallocation => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.margin_reallocation, v, now);
            }
        }
        _ => {}
    }
}

fn apply_place_order(p: &mut PlaceOrderAction, slot: &ActionSlot, value: Value, now: Time) {
    // Hyperliquid pre-sign orders carry no live inputs — nothing to enrich.
    let Some(li) = &mut p.live_inputs else {
        return;
    };
    match slot {
        ActionSlot::PerpPlaceLimitMarkPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.mark_price, d, now);
            }
        }
        ActionSlot::PerpPlaceLimitBestBidAsk => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.best_bid_ask, v, now);
            }
        }
        ActionSlot::PerpPlaceLimitOpenOrdersCount => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.open_orders_count, n as u32, now);
            }
        }
        ActionSlot::PerpPlaceLimitUserAccountState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_account_state, v, now);
            }
        }
        _ => {}
    }
}

fn apply_claim_funding(c: &mut ClaimFundingAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut c.live_inputs;
    if matches!(slot, ActionSlot::PerpClaimFundingClaimable) {
        if let Ok(v) = serde_json::from_value(value) {
            set_field(&mut li.claimable, v, now);
        }
    }
}

fn value_to_i256(v: &Value) -> Option<SignedI256> {
    use std::str::FromStr;
    match v {
        Value::String(s) => SignedI256::from_str(s).ok(),
        Value::Number(n) => n.as_i64().and_then(|i| SignedI256::try_from(i).ok()),
        _ => None,
    }
}
