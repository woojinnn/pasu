//! Lending 도메인의 walk + apply.
//!
//! 현재 wired: Borrow (5 slot), Supply (5 slot).
//! TODO: Withdraw, Repay, Liquidate, SwapRateMode, SetEMode, SetCollateral,
//!       DelegateBorrow.

use serde_json::Value;

use simulation_reducer::action::lending::{BorrowAction, LendingAction, SupplyAction};
use simulation_state::{Time, U256};

use crate::walker::{ActionSlot, StaleField, WalkStats};

use super::{push_if_stale, set_field, value_to_decimal, value_to_u256};

// ─────────────────────── walk ───────────────────────

pub(super) fn walk(
    la: &LendingAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    match la {
        LendingAction::Borrow(b) => walk_borrow(b, action_index, now, stale, stats),
        LendingAction::Supply(s) => walk_supply(s, action_index, now, stale, stats),
        // 나머지 lending 액션은 후속 패스
        _ => {}
    }
}

fn walk_borrow(
    b: &BorrowAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &b.live_inputs;
    push_if_stale(stale, stats, &li.reserve_state, now, action_index, ActionSlot::LendingBorrowReserveState);
    push_if_stale(stale, stats, &li.user_state_before, now, action_index, ActionSlot::LendingBorrowUserState);
    push_if_stale(stale, stats, &li.asset_price_usd, now, action_index, ActionSlot::LendingBorrowAssetPriceUsd);
    push_if_stale(stale, stats, &li.current_borrow_rate, now, action_index, ActionSlot::LendingBorrowCurrentRate);
    push_if_stale(stale, stats, &li.available_liquidity, now, action_index, ActionSlot::LendingBorrowAvailableLiquidity);
}

fn walk_supply(
    s: &SupplyAction,
    action_index: usize,
    now: Time,
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
) {
    let li = &s.live_inputs;
    push_if_stale(stale, stats, &li.reserve_state, now, action_index, ActionSlot::LendingSupplyReserveState);
    push_if_stale(stale, stats, &li.supply_apy, now, action_index, ActionSlot::LendingSupplySupplyApy);
    push_if_stale(stale, stats, &li.a_token_price_usd, now, action_index, ActionSlot::LendingSupplyATokenPriceUsd);
    push_if_stale(stale, stats, &li.eligible_as_collat, now, action_index, ActionSlot::LendingSupplyEligibleAsCollat);
    push_if_stale(stale, stats, &li.user_state_before, now, action_index, ActionSlot::LendingSupplyUserState);
}

// ─────────────────────── apply ───────────────────────

pub(super) fn apply(la: &mut LendingAction, slot: &ActionSlot, value: Value, now: Time) {
    match la {
        LendingAction::Borrow(b) => apply_borrow(b, slot, value, now),
        LendingAction::Supply(s) => apply_supply(s, slot, value, now),
        _ => {}
    }
}

fn apply_borrow(b: &mut BorrowAction, slot: &ActionSlot, value: Value, now: Time) {
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

fn apply_supply(s: &mut SupplyAction, slot: &ActionSlot, value: Value, now: Time) {
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
        // Borrow 슬롯이 들어와도 무시 (잘못된 dispatch 방어)
        _ => {}
    }
    // 미사용 변수 경고 회피
    let _ = (s, now);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_walk::{apply_value_to_action, walk_action_stale};
    use crate::walker::FieldLocation;

    use simulation_reducer::action::lending::{BorrowAction, BorrowLiveInputs, LendingVenue, ReserveState, UserLendingState};
    use simulation_reducer::action::{Action, ActionBody, ActionMeta, ActionNature};
    use simulation_state::{
        Address, ChainId, DataSource, Decimal, Duration, LiveField, OracleProvider, Price, RateMode,
        Time, TokenKey, TokenRef, U256,
    };

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

    fn mk_borrow_action(synced_at: u64) -> Action {
        let chain = ChainId::ethereum_mainnet();
        let aave_pool = Address::ZERO;
        let usdc = Address::ZERO;

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
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
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
                    gas_price: LiveField::new(U256::ZERO, DataSource::UserSupplied, Time::from_unix(synced_at)),
                    value: U256::ZERO,
                },
            },
            body: ActionBody::Lending(LendingAction::Borrow(borrow)),
        }
    }

    #[test]
    fn walks_borrow_live_inputs_when_stale() {
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
        apply_value_to_action(&mut action, &loc, Value::String("1.0001".into()), Time::from_unix(2_000_000));

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
        apply_value_to_action(&mut action, &loc, Value::String("12000000000000".into()), Time::from_unix(2_000_000));

        if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
            assert_eq!(b.live_inputs.available_liquidity.value, U256::from(12_000_000_000_000u64));
        } else {
            panic!("expected Borrow action");
        }
    }
}
