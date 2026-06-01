//! `SetEModeAction` reducer — `Aave V3` e-mode category selection.
//!
//! E-mode (efficiency mode) groups correlated assets so the user gets
//! higher LTV / liquidation threshold within the category at the cost of
//! restricting borrows to that category. `category_id = 0` disables e-mode.
//!
//! Flow (PDF §6.8):
//!
//! 1. Look up the `LendingAccount` — `PositionNotFound` when missing.
//! 2. Only `AaveV3` / `Spark` venues currently honour e-mode; reject
//!    others with `UnsupportedProtocol`.
//! 3. Reject when switching into a category whose `assets_in_category`
//!    does not cover every collateral / debt the wallet holds.
//! 4. Update `LendingAccount.emode` to the new category id.
//!
//! Token balances are unchanged.

use simulation_state::position::PositionKind;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::{LendingVenue, SetEModeAction};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::position_id;

impl Reducer for SetEModeAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;
        if !matches!(
            self.venue,
            LendingVenue::AaveV3 { .. } | LendingVenue::Spark { .. }
        ) {
            return Err(ReducerError::UnsupportedProtocol {
                action: "set_emode".into(),
                protocol: super::venue_tag(&self.venue).into(),
            });
        }

        let pid = position_id::for_venue(&self.venue);
        if !super::position_exists(state, &StateDelta::new(), &pid) {
            return Err(ReducerError::PositionNotFound(pid));
        }

        let pos = state
            .positions
            .iter()
            .find(|p| p.id == pid)
            .ok_or_else(|| ReducerError::PositionNotFound(pid.clone()))?;
        let PositionKind::LendingAccount(la) = &pos.kind else {
            return Err(ReducerError::Invariant(format!(
                "set_emode: position {pid} is not a LendingAccount"
            )));
        };

        // Step 3 — verify the new category covers every held asset (unless
        // disabling).
        if self.category_id != 0 {
            let allowed = &self.live_inputs.category_config.value.assets_in_category;
            for (token, _) in &la.collaterals {
                if !allowed.contains(token) {
                    return Err(ReducerError::Invariant(format!(
                        "set_emode rejected: collateral {:?} not in target category",
                        token.key
                    )));
                }
            }
            for (token, _, _) in &la.debts {
                if !allowed.contains(token) {
                    return Err(ReducerError::Invariant(format!(
                        "set_emode rejected: debt {:?} not in target category",
                        token.key
                    )));
                }
            }
        }

        let mut delta = StateDelta::new();
        let new_id = self.category_id;
        helpers::position::upsert_lending_account(state, &mut delta, &pid, |p| {
            if let PositionKind::LendingAccount(la) = &mut p.kind {
                la.emode = if new_id == 0 { None } else { Some(new_id) };
            }
        })?;

        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::{EModeConfig, LendingVenue, SetEModeLiveInputs, UserLendingState};
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::position::{LendingAccount, Position, PositionKind};
    use simulation_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, Time, VenueRef, U256,
    };
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::wallet::WalletId;
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

    fn weth_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
        })
    }

    fn aave_v3_venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    fn lending_pos(token: TokenRef) -> Position {
        let pid = super::position_id::for_venue(&aave_v3_venue());
        Position {
            id: pid,
            protocol: ProtocolRef::new("aave_v3"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::LendingAccount(LendingAccount {
                market: MarketRef {
                    symbol: "aave_v3".into(),
                    venue: VenueRef::new("aave_v3"),
                },
                collaterals: vec![(token, U256::from(1_000_u64))],
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

    fn state_with(token: TokenRef) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions.push(lending_pos(token));
        s
    }

    fn action(category_id: u8, assets: Vec<TokenRef>) -> SetEModeAction {
        SetEModeAction {
            venue: aave_v3_venue(),
            category_id,
            live_inputs: SetEModeLiveInputs {
                category_config: LiveField::new(
                    EModeConfig {
                        ltv_bp: 9_500,
                        liquidation_threshold_bp: 9_700,
                        liquidation_bonus_bp: 200,
                        price_source: None,
                        assets_in_category: assets,
                        category: None,
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

    /// Happy path: switching into category 1 with USDC in the allowed list.
    #[test]
    fn set_emode_happy_path() {
        let state = state_with(usdc_ref());
        let delta = action(1, vec![usdc_ref()]).apply(&state, &ctx()).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
    }

    /// Disable (category 0) always succeeds.
    #[test]
    fn set_emode_disable_succeeds() {
        let state = state_with(usdc_ref());
        let delta = action(0, vec![]).apply(&state, &ctx()).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
    }

    /// Switching to a category that does not cover the wallet's collateral
    /// is rejected.
    #[test]
    fn set_emode_uncovered_collateral_is_invariant() {
        let state = state_with(weth_ref());
        let err = action(1, vec![usdc_ref()])
            .apply(&state, &ctx())
            .unwrap_err();
        assert!(
            matches!(err, ReducerError::Invariant(msg) if msg.contains("not in target category"))
        );
    }

    /// Non-Aave venue rejects e-mode toggling.
    #[test]
    fn set_emode_non_aave_is_unsupported() {
        let state = state_with(usdc_ref());
        let mut a = action(1, vec![usdc_ref()]);
        a.venue = LendingVenue::CompoundV2 {
            chain: ChainId::ethereum_mainnet(),
            comptroller: Address::from_str("0x3d9819210a31b4961b30ef54be2aed79b9c9cd3b").unwrap(),
        };
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::UnsupportedProtocol { .. }));
    }
}
