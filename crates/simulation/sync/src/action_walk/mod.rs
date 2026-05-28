//! Action 트리 walker + apply — 도메인별 파일 디스패치.
//!
//! `WalletState` walker 와 평행한 모듈이지만 대상이 `Action.body.*.live_inputs`.
//!
//! 구조:
//! * 진입점:        [`walk_action_stale`], [`apply_value_to_action`] (이 파일)
//! * 도메인 dispatch: [`walk_body`] / [`body_at_index_mut`] (이 파일)
//! * 도메인별 본문:  `token.rs`, `amm.rs`, `lending.rs`, `airdrop.rs`,
//!                  `launchpad.rs`, `perp.rs`
//! * 공유 헬퍼:      [`push_if_stale`], [`set_field`], [`value_to_decimal`],
//!                  [`value_to_u256`]
//!
//! 현재 wire-up 된 도메인: lending (borrow + supply). 나머지는 빈 함수 stub.

use serde_json::Value;

use simulation_reducer::action::{Action, ActionBody};
use simulation_state::{LiveField, Time};

use crate::walker::{ActionSlot, FieldLocation, StaleField, WalkStats};

pub mod airdrop;
pub mod amm;
pub mod launchpad;
pub mod lending;
pub mod perp;
pub mod token;

// ─────────────────────── walk 진입점 ───────────────────────

/// `action` 안의 stale LiveField 들 수집. 단일 액션이면 action_index=0,
/// `Multicall` 자식들은 0..N 순서로 부여.
pub fn walk_action_stale(action: &Action, now: Time) -> (Vec<StaleField>, WalkStats) {
    let mut stale = Vec::new();
    let mut stats = WalkStats::default();
    walk_body(&action.body, 0, now, &mut stale, &mut stats);
    (stale, stats)
}

fn walk_body(
    body: &ActionBody,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match body {
        ActionBody::Token(t) => token::walk(t, action_index, now, stale, stats),
        ActionBody::Amm(a) => amm::walk(a, action_index, now, stale, stats),
        ActionBody::Lending(la) => lending::walk(la, action_index, now, stale, stats),
        ActionBody::Airdrop(a) => airdrop::walk(a, action_index, now, stale, stats),
        ActionBody::Launchpad(l) => launchpad::walk(l, action_index, now, stale, stats),
        ActionBody::Perp(p) => perp::walk(p, action_index, now, stale, stats),
        ActionBody::Multicall { actions } => {
            for (i, child) in actions.iter().enumerate() {
                walk_body(child, i, now, stale, stats);
            }
        }
        ActionBody::Unknown { .. } => {}
    }
}

// ─────────────────────── apply 진입점 ───────────────────────

/// fetched `value` 를 Action 의 해당 LiveField 슬롯에 in-place 로 적용.
/// `slot` variant 별 dispatch. 알 수 없는 슬롯이거나 값 형식 mismatch 면 no-op.
pub fn apply_value_to_action(
    action: &mut Action,
    location: &FieldLocation,
    value: Value,
    now: Time,
) {
    let FieldLocation::Action { action_index, slot } = location else {
        return; // wallet 측 location 은 apply_value (orchestrator) 가 처리
    };

    let body = body_at_index_mut(&mut action.body, *action_index);
    let Some(body) = body else { return };

    match body {
        ActionBody::Token(t) => token::apply(t, slot, value, now),
        ActionBody::Amm(a) => amm::apply(a, slot, value, now),
        ActionBody::Lending(la) => lending::apply(la, slot, value, now),
        ActionBody::Airdrop(a) => airdrop::apply(a, slot, value, now),
        ActionBody::Launchpad(l) => launchpad::apply(l, slot, value, now),
        ActionBody::Perp(p) => perp::apply(p, slot, value, now),
        ActionBody::Multicall { .. } | ActionBody::Unknown { .. } => {}
    }
}

pub(crate) fn body_at_index_mut(body: &mut ActionBody, index: usize) -> Option<&mut ActionBody> {
    match body {
        ActionBody::Multicall { actions } => actions.get_mut(index),
        single if index == 0 => Some(single),
        _ => None,
    }
}

// ─────────────────────── 공유 헬퍼 (도메인 파일들이 사용) ───────────────────────

pub(crate) fn push_if_stale<T>(
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
    field: &LiveField<T>,
    now: Time,
    action_index: usize,
    slot: ActionSlot,
) {
    stats.total_live_fields += 1;
    if field.is_stale(now) {
        stats.stale_count += 1;
        stale.push(StaleField {
            location: FieldLocation::Action { action_index, slot },
            source: field.source.clone(),
            synced_at: field.synced_at,
        });
    } else {
        stats.fresh_count += 1;
    }
}

pub(crate) fn set_field<T>(field: &mut LiveField<T>, value: T, now: Time) {
    field.value = value;
    field.synced_at = now;
    field.confidence = Some(simulation_state::Confidence::fresh());
}

pub(crate) fn value_to_decimal(v: &Value) -> Option<simulation_state::Decimal> {
    match v {
        Value::String(s) => Some(simulation_state::Decimal::new(s.clone())),
        Value::Number(n) => Some(simulation_state::Decimal::new(n.to_string())),
        _ => None,
    }
}

pub(crate) fn value_to_u256(v: &Value) -> Option<simulation_state::U256> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    simulation_state::U256::from_str_radix(&s, 10).ok()
}
