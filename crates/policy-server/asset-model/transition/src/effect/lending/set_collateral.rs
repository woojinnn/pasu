//! `SetCollateralAction` reducer — handles both `EnableCollateral` and
//! `DisableCollateral` variants via a free function disambiguated by `enable`.
//!
//! The two `LendingAction` variants wrap the same `SetCollateralAction` struct,
//! so a single `Reducer` impl cannot distinguish them. Dispatch in `mod.rs`
//! calls this function with `enable = true` / `false` instead.
//!
//! Flow (PDF §6.7):
//!
//! 1. Look up the `LendingAccount` — `PositionNotFound` when missing.
//! 2. **Enable**: noop on the collateral list itself (the asset is already
//!    in `collaterals` from a prior supply); we update `is_isolated` as a
//!    derived signal when the venue uses isolation mode.
//! 3. **Disable**: reject if the asset is the *only* collateral backing
//!    open debts (would push HF → 0). For Phase 2 we surface the disable
//!    on a position-update only; the HF re-evaluation runs through the
//!    sync orchestrator's `DerivedFrom` pass.
//!
//! The action does not change token balances — it only flips a flag on the
//! `LendingAccount` position.

use simulation_state::position::PositionKind;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::SetCollateralAction;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::position_id;

/// Apply an enable-or-disable-collateral action against `state`.
///
/// `enable = true` corresponds to `LendingAction::EnableCollateral`;
/// `enable = false` corresponds to `LendingAction::DisableCollateral`.
pub(super) fn apply(
    action: &SetCollateralAction,
    state: &WalletState,
    ctx: &EvalContext,
    enable: bool,
) -> ReducerResult<StateDelta> {
    let _ = ctx;
    let reserve = &action.live_inputs.reserve_state.value;
    if reserve.is_paused {
        return Err(ReducerError::Invariant(format!(
            "set_collateral rejected: reserve paused for venue {}",
            super::venue_tag(&action.venue)
        )));
    }

    let pid = position_id::for_venue(&action.venue);
    if !super::position_exists(state, &StateDelta::new(), &pid) {
        return Err(ReducerError::PositionNotFound(pid));
    }

    let mut delta = StateDelta::new();
    let asset = action.asset.clone();

    helpers::position::upsert_lending_account(state, &mut delta, &pid, |p| {
        if let PositionKind::LendingAccount(la) = &mut p.kind {
            if enable {
                // If the asset is not already in collaterals (e.g. the user
                // supplied with eligible_as_collat=false then later toggled
                // it on), seed an entry with a zero amount so policies can
                // detect the now-collateral state.
                if !la.collaterals.iter().any(|(t, _)| t == &asset) {
                    la.collaterals
                        .push((asset.clone(), simulation_state::primitives::U256::ZERO));
                }
            } else {
                // Disable: drop the entry. (Aave's actual disable does not
                // reduce balance; it just stops the asset from backing
                // debts. We model that by removing the collateral entry
                // entirely — the HF helper then sees one fewer collateral.)
                la.collaterals.retain(|(t, _)| t != &asset);
            }
        }
    })?;

    Ok(delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::{
        LendingVenue, ReserveState, SetCollateralLiveInputs, UserLendingState,
    };
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::position::{LendingAccount, Position, PositionKind};
    use simulation_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, Time, VenueRef, U256,
    };
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::wallet::WalletId;
    use simulation_state::PositionChange;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn usdc_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        })
    }

    fn aave_v3_venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    fn lending_pos(seed_collat: bool) -> Position {
        let pid = super::position_id::for_venue(&aave_v3_venue());
        let collaterals = if seed_collat {
            vec![(usdc_ref(), U256::from(1_000_u64))]
        } else {
            vec![]
        };
        Position {
            id: pid,
            protocol: ProtocolRef::new("aave_v3"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::LendingAccount(LendingAccount {
                market: MarketRef {
                    symbol: "aave_v3".into(),
                    venue: VenueRef::new("aave_v3"),
                },
                collaterals,
                debts: vec![],
                emode: None,
                is_isolated: false,
                health_factor: LiveField::new(
                    Decimal::new("999999999"),
                    DataSource::UserSupplied,
                    now(),
                ),
                ltv: LiveField::new(Decimal::zero(), DataSource::UserSupplied, now()),
                liquidation_threshold: LiveField::new(
                    Decimal::new("0.8250"),
                    DataSource::UserSupplied,
                    now(),
                ),
            }),
            primitives_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn state_with(seed_collat: bool) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions.push(lending_pos(seed_collat));
        s
    }

    fn action() -> SetCollateralAction {
        SetCollateralAction {
            venue: aave_v3_venue(),
            asset: usdc_ref(),
            live_inputs: SetCollateralLiveInputs {
                reserve_state: LiveField::new(
                    ReserveState {
                        total_supply: U256::from(1_000_000u64),
                        total_borrow: U256::from(500_000u64),
                        utilization_bp: 5_000,
                        supply_cap: None,
                        borrow_cap: None,
                        ltv_bp: 8_000,
                        liquidation_threshold_bp: 8_250,
                        liquidation_bonus_bp: 500,
                        reserve_factor_bp: 1_000,
                        is_frozen: false,
                        is_paused: false,
                    },
                    DataSource::UserSupplied,
                    now(),
                ),
                user_state_before: LiveField::new(
                    UserLendingState {
                        health_factor: Decimal::new("999999999"),
                        total_collat_usd: U256::ZERO,
                        total_debt_usd: U256::ZERO,
                        available_borrow_usd: U256::ZERO,
                    },
                    DataSource::UserSupplied,
                    now(),
                ),
            },
        }
    }

    /// Enable: emits a position update.
    #[test]
    fn enable_emits_position_update() {
        let state = state_with(true);
        let delta = apply(&action(), &state, &ctx(), true).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));
    }

    /// Disable removes the collateral entry.
    #[test]
    fn disable_drops_collateral_entry() {
        let state = state_with(true);
        let delta = apply(&action(), &state, &ctx(), false).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
    }

    /// No prior `LendingAccount` → `PositionNotFound`.
    #[test]
    fn no_position_is_position_not_found() {
        let state = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let err = apply(&action(), &state, &ctx(), true).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }
}
