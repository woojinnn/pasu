//! Wired: Supply, Withdraw, Borrow, Repay, `SwapRateMode`, `SetEMode`,
//!        `EnableCollateral`, `DisableCollateral`, Liquidate.

use serde_json::Value;

use policy_state::Time;
use policy_transition::action::lending::{
    BorrowAction, LendingAction, LiquidateAction, RepayAction, SetCollateralAction, SetEModeAction,
    SupplyAction, SwapRateModeAction, WithdrawAction,
};

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
        LendingAction::Supply(s) => walk_supply(s, action_index, now, stale, stats),
        LendingAction::Withdraw(w) => walk_withdraw(w, action_index, now, stale, stats),
        LendingAction::Borrow(b) => walk_borrow(b, action_index, now, stale, stats),
        LendingAction::Repay(r) => walk_repay(r, action_index, now, stale, stats),
        LendingAction::SwapRateMode(s) => walk_swap_rate(s, action_index, now, stale, stats),
        LendingAction::SetEMode(s) => walk_set_emode(s, action_index, now, stale, stats),
        LendingAction::EnableCollateral(s) | LendingAction::DisableCollateral(s) => {
            walk_set_collat(s, action_index, now, stale, stats);
        }
        LendingAction::Liquidate(l) => walk_liquidate(l, action_index, now, stale, stats),
        LendingAction::DelegateBorrow(_) => {} // no live_inputs
        LendingAction::SetAuthorization(_) => {} // no live_inputs
        LendingAction::BuyCollateral(_) => {}  // no live_inputs
        LendingAction::PeripheryOperation(_) => {} // no live_inputs
    }
}

fn walk_supply(
    s: &SupplyAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &s.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.reserve_state,
        now,
        ix,
        ActionSlot::LendingSupplyReserveState,
    );
    push_if_stale(
        st,
        sx,
        &li.supply_apy,
        now,
        ix,
        ActionSlot::LendingSupplySupplyApy,
    );
    push_if_stale(
        st,
        sx,
        &li.a_token_price_usd,
        now,
        ix,
        ActionSlot::LendingSupplyATokenPriceUsd,
    );
    push_if_stale(
        st,
        sx,
        &li.eligible_as_collat,
        now,
        ix,
        ActionSlot::LendingSupplyEligibleAsCollat,
    );
    push_if_stale(
        st,
        sx,
        &li.user_state_before,
        now,
        ix,
        ActionSlot::LendingSupplyUserState,
    );
}

fn walk_withdraw(
    w: &WithdrawAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &w.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.reserve_state,
        now,
        ix,
        ActionSlot::LendingWithdrawReserveState,
    );
    push_if_stale(
        st,
        sx,
        &li.available_to_withdraw,
        now,
        ix,
        ActionSlot::LendingWithdrawAvailableToWithdraw,
    );
    push_if_stale(
        st,
        sx,
        &li.user_state_before,
        now,
        ix,
        ActionSlot::LendingWithdrawUserState,
    );
}

fn walk_borrow(
    b: &BorrowAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &b.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.reserve_state,
        now,
        ix,
        ActionSlot::LendingBorrowReserveState,
    );
    push_if_stale(
        st,
        sx,
        &li.user_state_before,
        now,
        ix,
        ActionSlot::LendingBorrowUserState,
    );
    push_if_stale(
        st,
        sx,
        &li.asset_price_usd,
        now,
        ix,
        ActionSlot::LendingBorrowAssetPriceUsd,
    );
    push_if_stale(
        st,
        sx,
        &li.current_borrow_rate,
        now,
        ix,
        ActionSlot::LendingBorrowCurrentRate,
    );
    push_if_stale(
        st,
        sx,
        &li.available_liquidity,
        now,
        ix,
        ActionSlot::LendingBorrowAvailableLiquidity,
    );
}

fn walk_repay(r: &RepayAction, ix: usize, now: Time, st: &mut Vec<StaleField>, sx: &mut WalkStats) {
    let li = &r.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.reserve_state,
        now,
        ix,
        ActionSlot::LendingRepayReserveState,
    );
    push_if_stale(
        st,
        sx,
        &li.current_debt,
        now,
        ix,
        ActionSlot::LendingRepayCurrentDebt,
    );
    push_if_stale(
        st,
        sx,
        &li.user_state_before,
        now,
        ix,
        ActionSlot::LendingRepayUserState,
    );
}

fn walk_swap_rate(
    s: &SwapRateModeAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &s.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.current_debts,
        now,
        ix,
        ActionSlot::LendingSwapRateModeCurrentDebts,
    );
    push_if_stale(
        st,
        sx,
        &li.rates,
        now,
        ix,
        ActionSlot::LendingSwapRateModeRates,
    );
}

fn walk_set_emode(
    s: &SetEModeAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &s.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.category_config,
        now,
        ix,
        ActionSlot::LendingSetEModeCategoryConfig,
    );
    push_if_stale(
        st,
        sx,
        &li.user_state_before,
        now,
        ix,
        ActionSlot::LendingSetEModeUserState,
    );
}

fn walk_set_collat(
    s: &SetCollateralAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &s.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.reserve_state,
        now,
        ix,
        ActionSlot::LendingSetCollateralReserveState,
    );
    push_if_stale(
        st,
        sx,
        &li.user_state_before,
        now,
        ix,
        ActionSlot::LendingSetCollateralUserState,
    );
}

fn walk_liquidate(
    l: &LiquidateAction,
    ix: usize,
    now: Time,
    st: &mut Vec<StaleField>,
    sx: &mut WalkStats,
) {
    let li = &l.live_inputs;
    push_if_stale(
        st,
        sx,
        &li.victim_state,
        now,
        ix,
        ActionSlot::LendingLiquidateVictimState,
    );
    push_if_stale(
        st,
        sx,
        &li.liquidation_bonus,
        now,
        ix,
        ActionSlot::LendingLiquidateBonus,
    );
    push_if_stale(
        st,
        sx,
        &li.debt_asset_price,
        now,
        ix,
        ActionSlot::LendingLiquidateDebtAssetPrice,
    );
    push_if_stale(
        st,
        sx,
        &li.collat_asset_price,
        now,
        ix,
        ActionSlot::LendingLiquidateCollatAssetPrice,
    );
}

// ─────────────────────── apply ───────────────────────

pub(super) fn apply(la: &mut LendingAction, slot: &ActionSlot, value: Value, now: Time) {
    match la {
        LendingAction::Supply(s) => apply_supply(s, slot, value, now),
        LendingAction::Withdraw(w) => apply_withdraw(w, slot, value, now),
        LendingAction::Borrow(b) => apply_borrow(b, slot, value, now),
        LendingAction::Repay(r) => apply_repay(r, slot, value, now),
        LendingAction::SwapRateMode(s) => apply_swap_rate(s, slot, value, now),
        LendingAction::SetEMode(s) => apply_set_emode(s, slot, value, now),
        LendingAction::EnableCollateral(s) | LendingAction::DisableCollateral(s) => {
            apply_set_collat(s, slot, value, now);
        }
        LendingAction::Liquidate(l) => apply_liquidate(l, slot, value, now),
        LendingAction::DelegateBorrow(_) => {}
        LendingAction::SetAuthorization(_) => {}
        LendingAction::BuyCollateral(_) => {}
        LendingAction::PeripheryOperation(_) => {}
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
        _ => {}
    }
}

fn apply_withdraw(w: &mut WithdrawAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut w.live_inputs;
    match slot {
        ActionSlot::LendingWithdrawReserveState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.reserve_state, v, now);
            }
        }
        ActionSlot::LendingWithdrawAvailableToWithdraw => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.available_to_withdraw, u, now);
            }
        }
        ActionSlot::LendingWithdrawUserState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_state_before, v, now);
            }
        }
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

fn apply_repay(r: &mut RepayAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut r.live_inputs;
    match slot {
        ActionSlot::LendingRepayReserveState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.reserve_state, v, now);
            }
        }
        ActionSlot::LendingRepayCurrentDebt => {
            if let Some(u) = value_to_u256(&value) {
                set_field(&mut li.current_debt, u, now);
            }
        }
        ActionSlot::LendingRepayUserState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_state_before, v, now);
            }
        }
        _ => {}
    }
}

fn apply_swap_rate(s: &mut SwapRateModeAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut s.live_inputs;
    match slot {
        ActionSlot::LendingSwapRateModeCurrentDebts => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.current_debts, v, now);
            }
        }
        ActionSlot::LendingSwapRateModeRates => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.rates, v, now);
            }
        }
        _ => {}
    }
}

fn apply_set_emode(s: &mut SetEModeAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut s.live_inputs;
    match slot {
        ActionSlot::LendingSetEModeCategoryConfig => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.category_config, v, now);
            }
        }
        ActionSlot::LendingSetEModeUserState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_state_before, v, now);
            }
        }
        _ => {}
    }
}

fn apply_set_collat(s: &mut SetCollateralAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut s.live_inputs;
    match slot {
        ActionSlot::LendingSetCollateralReserveState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.reserve_state, v, now);
            }
        }
        ActionSlot::LendingSetCollateralUserState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.user_state_before, v, now);
            }
        }
        _ => {}
    }
}

fn apply_liquidate(l: &mut LiquidateAction, slot: &ActionSlot, value: Value, now: Time) {
    let li = &mut l.live_inputs;
    match slot {
        ActionSlot::LendingLiquidateVictimState => {
            if let Ok(v) = serde_json::from_value(value) {
                set_field(&mut li.victim_state, v, now);
            }
        }
        ActionSlot::LendingLiquidateBonus => {
            if let Some(n) = value.as_u64() {
                set_field(&mut li.liquidation_bonus, n as u32, now);
            }
        }
        ActionSlot::LendingLiquidateDebtAssetPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.debt_asset_price, d, now);
            }
        }
        ActionSlot::LendingLiquidateCollatAssetPrice => {
            if let Some(d) = value_to_decimal(&value) {
                set_field(&mut li.collat_asset_price, d, now);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_walk::{apply_value_to_action, walk_action_stale};
    use crate::walker::FieldLocation;

    use policy_state::{
        Address, ChainId, DataSource, Decimal, Duration, LiveField, OracleProvider, Price,
        RateMode, Time, TokenKey, TokenRef, U256,
    };
    use policy_transition::action::lending::{
        BorrowAction, BorrowLiveInputs, LendingVenue, ReserveState, UserLendingState,
    };
    use policy_transition::action::{Action, ActionBody, ActionMeta, ActionNature};

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
    fn dummy_user() -> UserLendingState {
        UserLendingState {
            health_factor: Decimal::from("0"),
            total_collat_usd: U256::ZERO,
            total_debt_usd: U256::ZERO,
            available_borrow_usd: U256::ZERO,
        }
    }

    fn mk_borrow(synced_at: u64) -> Action {
        let chain = ChainId::ethereum_mainnet();
        let aave = Address::ZERO;
        let usdc = Address::ZERO;
        let stale_src = DataSource::OnchainView {
            chain: chain.clone(),
            contract: aave,
            function: "x".into(),
            decoder_id: "x".into(),
        };
        Action {
            meta: ActionMeta {
                submitted_at: Time::from_unix(synced_at),
                submitter: Address::ZERO,
                nature: ActionNature::OnchainTx {
                    chain: chain.clone(),
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
            body: ActionBody::Lending(LendingAction::Borrow(BorrowAction {
                venue: LendingVenue::AaveV3 {
                    chain: chain.clone(),
                    pool: aave,
                    market_id: None,
                },
                asset: TokenRef {
                    key: TokenKey::Erc20 {
                        chain,
                        address: usdc,
                    },
                },
                amount: U256::from(500u64),
                rate_mode: RateMode::Variable,
                on_behalf_of: None,
                live_inputs: BorrowLiveInputs {
                    reserve_state: LiveField::new(
                        dummy_reserve(),
                        stale_src.clone(),
                        Time::from_unix(synced_at),
                    )
                    .with_ttl(Duration::from_secs(60)),
                    user_state_before: LiveField::new(
                        dummy_user(),
                        stale_src.clone(),
                        Time::from_unix(synced_at),
                    )
                    .with_ttl(Duration::from_secs(60)),
                    asset_price_usd: LiveField::new(
                        Price::from("0"),
                        DataSource::OracleFeed {
                            provider: OracleProvider::Chainlink,
                            feed_id: "USDC/USD".into(),
                        },
                        Time::from_unix(synced_at),
                    )
                    .with_ttl(Duration::from_secs(60)),
                    current_borrow_rate: LiveField::new(
                        Decimal::from("0"),
                        stale_src.clone(),
                        Time::from_unix(synced_at),
                    )
                    .with_ttl(Duration::from_secs(60)),
                    available_liquidity: LiveField::new(
                        U256::ZERO,
                        stale_src,
                        Time::from_unix(synced_at),
                    )
                    .with_ttl(Duration::from_secs(60)),
                },
            })),
        }
    }

    #[test]
    fn walks_borrow_when_stale() {
        let action = mk_borrow(1);
        let (stale, stats) = walk_action_stale(&action, Time::from_unix(1_000_000));
        assert_eq!(stats.total_live_fields, 5);
        assert_eq!(stats.stale_count, 5);
        for s in &stale {
            assert!(matches!(
                s.location,
                FieldLocation::Action {
                    action_index: 0,
                    ..
                }
            ));
        }
    }

    #[test]
    fn apply_asset_price_to_borrow() {
        let mut action = mk_borrow(1);
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
        } else {
            panic!("expected Borrow");
        }
    }
}
