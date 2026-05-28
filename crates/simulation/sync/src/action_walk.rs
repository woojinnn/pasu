//! Action 트리 walker + apply.
//!
//! `WalletState` walker 와 평행한 모듈이지만 대상이 `Action` 의 `live_inputs`.
//!
//! Phase 1 scope: lending borrow / supply 의 5+5 슬롯만.
//! 다른 액션은 같은 패턴 (walker fn + apply fn) 추가로 양산.

use serde_json::Value;

use simulation_reducer::action::lending::LendingAction;
use simulation_reducer::action::{Action, ActionBody};
use simulation_state::{LiveField, Time};

use crate::walker::{ActionSlot, FieldLocation, StaleField, WalkStats};

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
        ActionBody::Lending(la) => walk_lending(la, action_index, now, stale, stats),
        ActionBody::Multicall { actions } => {
            for (i, child) in actions.iter().enumerate() {
                walk_body(child, i, now, stale, stats);
            }
        }
        // 나머지 액션 도메인 (token/amm/airdrop/launchpad/perp) 은 후속 패스에서.
        ActionBody::Token(_)
        | ActionBody::Amm(_)
        | ActionBody::Airdrop(_)
        | ActionBody::Launchpad(_)
        | ActionBody::Perp(_)
        | ActionBody::Unknown { .. } => {}
    }
}

fn walk_lending(
    la: &LendingAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match la {
        LendingAction::Borrow(b) => {
            let li = &b.live_inputs;
            push_if_stale(
                stale,
                stats,
                &li.reserve_state,
                now,
                action_index,
                ActionSlot::LendingBorrowReserveState,
            );
            push_if_stale(
                stale,
                stats,
                &li.user_state_before,
                now,
                action_index,
                ActionSlot::LendingBorrowUserState,
            );
            push_if_stale(
                stale,
                stats,
                &li.asset_price_usd,
                now,
                action_index,
                ActionSlot::LendingBorrowAssetPriceUsd,
            );
            push_if_stale(
                stale,
                stats,
                &li.current_borrow_rate,
                now,
                action_index,
                ActionSlot::LendingBorrowCurrentRate,
            );
            push_if_stale(
                stale,
                stats,
                &li.available_liquidity,
                now,
                action_index,
                ActionSlot::LendingBorrowAvailableLiquidity,
            );
        }
        LendingAction::Supply(s) => {
            let li = &s.live_inputs;
            push_if_stale(
                stale,
                stats,
                &li.reserve_state,
                now,
                action_index,
                ActionSlot::LendingSupplyReserveState,
            );
            push_if_stale(
                stale,
                stats,
                &li.supply_apy,
                now,
                action_index,
                ActionSlot::LendingSupplySupplyApy,
            );
            push_if_stale(
                stale,
                stats,
                &li.a_token_price_usd,
                now,
                action_index,
                ActionSlot::LendingSupplyATokenPriceUsd,
            );
            push_if_stale(
                stale,
                stats,
                &li.eligible_as_collat,
                now,
                action_index,
                ActionSlot::LendingSupplyEligibleAsCollat,
            );
            push_if_stale(
                stale,
                stats,
                &li.user_state_before,
                now,
                action_index,
                ActionSlot::LendingSupplyUserState,
            );
        }
        // 나머지 lending 액션 (Withdraw, Repay, ...) 은 후속.
        _ => {}
    }
}

fn push_if_stale<T>(
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

// ─────────────────────────── apply ───────────────────────────

/// fetched `value` 를 Action 의 해당 LiveField 슬롯에 in-place 로 적용.
/// `slot` variant 별 dispatch. 알 수 없는 슬롯이거나 값 형식 mismatch 면 no-op.
pub fn apply_value_to_action(action: &mut Action, location: &FieldLocation, value: Value, now: Time) {
    let FieldLocation::Action { action_index, slot } = location else {
        return; // wallet 측 location 은 apply_value (orchestrator) 가 처리
    };

    let body = body_at_index_mut(&mut action.body, *action_index);
    let Some(body) = body else { return };

    match (body, slot) {
        (ActionBody::Lending(LendingAction::Borrow(b)), s) => {
            apply_borrow_slot(b, s, value, now);
        }
        (ActionBody::Lending(LendingAction::Supply(s_a)), s) => {
            apply_supply_slot(s_a, s, value, now);
        }
        _ => {}
    }
}

fn body_at_index_mut(body: &mut ActionBody, index: usize) -> Option<&mut ActionBody> {
    match body {
        ActionBody::Multicall { actions } => actions.get_mut(index),
        single if index == 0 => Some(single),
        _ => None,
    }
}

fn apply_borrow_slot(
    b: &mut simulation_reducer::action::lending::BorrowAction,
    slot: &ActionSlot,
    value: Value,
    now: Time,
) {
    let li = &mut b.live_inputs;
    match slot {
        ActionSlot::LendingBorrowReserveState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.reserve_state, v, now);
            }
        }
        ActionSlot::LendingBorrowUserState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_state_before, v, now);
            }
        }
        ActionSlot::LendingBorrowAssetPriceUsd => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.asset_price_usd, d, now);
            }
        }
        ActionSlot::LendingBorrowCurrentRate => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.current_borrow_rate, d, now);
            }
        }
        ActionSlot::LendingBorrowAvailableLiquidity => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.available_liquidity, u, now);
            }
        }
        _ => {}
    }
}

fn apply_supply_slot(
    s: &mut simulation_reducer::action::lending::SupplyAction,
    slot: &ActionSlot,
    value: Value,
    now: Time,
) {
    let li = &mut s.live_inputs;
    match slot {
        ActionSlot::LendingSupplyReserveState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.reserve_state, v, now);
            }
        }
        ActionSlot::LendingSupplySupplyApy => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.supply_apy, d, now);
            }
        }
        ActionSlot::LendingSupplyATokenPriceUsd => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.a_token_price_usd, d, now);
            }
        }
        ActionSlot::LendingSupplyEligibleAsCollat => {
            if let Value::Bool(b) = value {
                set_field(&mut li.eligible_as_collat, b, now);
            }
        }
        ActionSlot::LendingSupplyUserState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_state_before, v, now);
            }
        }
        _ => {}
    }
}

fn set_field<T>(field: &mut LiveField<T>, value: T, now: Time) {
    field.value = value;
    field.synced_at = now;
    field.confidence = Some(simulation_state::Confidence::fresh());
}

fn value_to_decimal(v: &Value) -> Option<simulation_state::Decimal> {
    match v {
        Value::String(s) => Some(simulation_state::Decimal::new(s.clone())),
        Value::Number(n) => Some(simulation_state::Decimal::new(n.to_string())),
        _ => None,
    }
}

fn value_to_u256(v: &Value) -> Option<simulation_state::U256> {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    simulation_state::U256::from_str_radix(&s, 10).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_reducer::action::lending::{BorrowAction, BorrowLiveInputs};
    use simulation_state::{
        Address, ChainId, DataSource, Decimal, Duration, LiveField, OracleProvider, Price, Time,
        U256,
    };

    fn mk_borrow_action(synced_at: u64) -> Action {
        use simulation_reducer::action::lending::{LendingAction, LendingVenue, ReserveState, UserLendingState};
        use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature};
        use simulation_state::{RateMode, TokenKey, TokenRef};

        fn dummy_reserve() -> ReserveState {
            ReserveState {
                total_supply: U256::ZERO,
                total_borrow: U256::ZERO,
                utilization_bp: 0,
                supply_cap: None,
                borrow_cap: None,
                ltv_bp: 0,
                liquidation_threshold_bp: 0,
                liquidation_bonus_bp: 0,
                reserve_factor_bp: 0,
                is_frozen: false,
                is_paused: false,
            }
        }
        fn dummy_user_state() -> UserLendingState {
            UserLendingState {
                health_factor: Decimal::from("0"),
                total_collat_usd: U256::ZERO,
                total_debt_usd: U256::ZERO,
                available_borrow_usd: U256::ZERO,
            }
        }

        let chain = ChainId::ethereum_mainnet();
        let aave_pool = Address::ZERO;
        let usdc = Address::ZERO;

        let stale_source = DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "USDC/USD".into(),
        };

        let borrow = BorrowAction {
            venue: LendingVenue::AaveV3 {
                chain: chain.clone(),
                pool: aave_pool,
                market_id: None,
            },
            asset: TokenRef {
                key: TokenKey::Erc20 { chain: chain.clone(), address: usdc },
            },
            amount: U256::from(500u64),
            rate_mode: RateMode::Variable,
            on_behalf_of: None,
            live_inputs: BorrowLiveInputs {
                reserve_state: LiveField::new(
                    dummy_reserve(),
                    DataSource::OnchainView {
                        chain: chain.clone(),
                        contract: aave_pool,
                        function: "getReserveData(address)".into(),
                        decoder_id: "aave_reserve_data".into(),
                    },
                    Time::from_unix(synced_at),
                )
                .with_ttl(Duration::from_secs(60)),
                user_state_before: LiveField::new(
                    dummy_user_state(),
                    DataSource::OnchainView {
                        chain: chain.clone(),
                        contract: aave_pool,
                        function: "getUserAccountData(address)".into(),
                        decoder_id: "aave_user_data".into(),
                    },
                    Time::from_unix(synced_at),
                )
                .with_ttl(Duration::from_secs(60)),
                asset_price_usd: LiveField::new(
                    Price::from("0.0"),
                    stale_source.clone(),
                    Time::from_unix(synced_at),
                )
                .with_ttl(Duration::from_secs(60)),
                current_borrow_rate: LiveField::new(
                    Decimal::from("0.0"),
                    DataSource::OnchainView {
                        chain: chain.clone(),
                        contract: aave_pool,
                        function: "getReserveData(address)".into(),
                        decoder_id: "u256".into(),
                    },
                    Time::from_unix(synced_at),
                )
                .with_ttl(Duration::from_secs(60)),
                available_liquidity: LiveField::new(
                    U256::ZERO,
                    DataSource::OnchainView {
                        chain: chain.clone(),
                        contract: usdc,
                        function: "balanceOf(address)".into(),
                        decoder_id: "erc20_balance".into(),
                    },
                    Time::from_unix(synced_at),
                )
                .with_ttl(Duration::from_secs(60)),
            },
        };

        Action {
            meta: ActionMeta {
                submitted_at: Time::from_unix(synced_at),
                submitter: Address::ZERO,
                nature: ActionNature::OnchainTx {
                    chain,
                    nonce: 0,
                    gas_limit: U256::from(200_000u64),
                    gas_price: LiveField::new(
                        U256::ZERO,
                        DataSource::UserSupplied,
                        Time::from_unix(synced_at),
                    ),
                    value: U256::ZERO,
                },
            },
            body: ActionBody::Lending(LendingAction::Borrow(borrow)),
        }
    }

    #[test]
    fn walks_borrow_live_inputs_when_stale() {
        // synced_at=1, ttl=60s, now=1_000_000 → 5개 모두 stale
        let action = mk_borrow_action(1);
        let (stale, stats) = walk_action_stale(&action, Time::from_unix(1_000_000));
        assert_eq!(stats.total_live_fields, 5);
        assert_eq!(stats.stale_count, 5);
        assert_eq!(stale.len(), 5);
        for s in &stale {
            assert!(matches!(s.location, FieldLocation::Action { action_index: 0, .. }));
        }
    }

    #[test]
    fn walk_returns_empty_when_all_fresh() {
        // synced_at=999_000, ttl=60s, now=999_010 → 모두 fresh
        let action = mk_borrow_action(999_000);
        let (stale, stats) = walk_action_stale(&action, Time::from_unix(999_010));
        assert_eq!(stats.total_live_fields, 5);
        assert_eq!(stats.fresh_count, 5);
        assert_eq!(stats.stale_count, 0);
        assert_eq!(stale.len(), 0);
    }

    #[test]
    fn apply_writes_asset_price_to_borrow() {
        let mut action = mk_borrow_action(1);
        let loc = FieldLocation::Action {
            action_index: 0,
            slot: ActionSlot::LendingBorrowAssetPriceUsd,
        };
        apply_value_to_action(
            &mut action,
            &loc,
            Value::String("1.0001".into()),
            Time::from_unix(2_000_000),
        );

        if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
            assert_eq!(b.live_inputs.asset_price_usd.value.as_str(), "1.0001");
            assert_eq!(b.live_inputs.asset_price_usd.synced_at.as_unix(), 2_000_000);
        } else {
            panic!("expected Borrow action");
        }
    }

    #[test]
    fn apply_writes_available_liquidity_as_u256() {
        let mut action = mk_borrow_action(1);
        let loc = FieldLocation::Action {
            action_index: 0,
            slot: ActionSlot::LendingBorrowAvailableLiquidity,
        };
        apply_value_to_action(
            &mut action,
            &loc,
            Value::String("12000000000000".into()),
            Time::from_unix(2_000_000),
        );

        if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
            assert_eq!(
                b.live_inputs.available_liquidity.value,
                U256::from(12_000_000_000_000u64),
            );
        } else {
            panic!("expected Borrow action");
        }
    }
}
