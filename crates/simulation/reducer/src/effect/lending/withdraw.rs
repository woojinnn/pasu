//! `WithdrawAction` reducer — withdraw a previously supplied asset.
//!
//! Flow (PDF §6.2):
//!
//! 1. Validate `live_inputs.reserve_state` — reject when paused. (Frozen
//!    reserves still permit withdrawals, matching Aave V3 governance
//!    convention.)
//! 2. Look up the wallet's `LendingAccount` for this venue — `Invariant`
//!    when missing (cannot withdraw without prior supply).
//! 3. `amount == U256::MAX` → max-withdraw: substitute the current
//!    collateral balance for this asset (`live_inputs.available_to_withdraw`
//!    is the authoritative source when the orchestrator wires it through,
//!    otherwise the `LendingAccount.collaterals` tuple).
//! 4. Reduce the collateral entry; emit `PositionChange::Update`.
//! 5. `balance::credit` the underlying asset to `recipient`.
//!
//! HF / LTV recompute is left to the sync orchestrator's `DerivedFrom`
//! cycle — `recompute_health_factor` lives in `helpers::derived` and runs
//! as a follow-up pass; surfacing it inline would require building the
//! per-token price tables which `WithdrawLiveInputs` does not yet carry.

use simulation_state::position::PositionKind;
use simulation_state::primitives::U256;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::WithdrawAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::position_id;

impl Reducer for WithdrawAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;
        let reserve = &self.live_inputs.reserve_state.value;

        if reserve.is_paused {
            return Err(ReducerError::Invariant(format!(
                "withdraw rejected: reserve paused for venue {}",
                super::venue_tag(&self.venue),
            )));
        }

        let pid = position_id::for_venue(&self.venue);
        if !super::position_exists(state, &StateDelta::new(), &pid) {
            return Err(ReducerError::PositionNotFound(pid));
        }

        // Resolve max-withdraw via the LiveField hint; the actual reduction
        // is enforced against the on-position collateral entry via
        // `reduce_collateral` so the user can never withdraw more than they
        // hold even when the LiveField is stale.
        let intended_amount = if self.amount == U256::MAX {
            self.live_inputs.available_to_withdraw.value
        } else {
            self.amount
        };

        let mut delta = StateDelta::new();

        // Reduce the collateral entry first so a downstream balance::credit
        // failure cannot leave us in a state where the LendingAccount under-
        // counts the withdrawal.
        let asset = self.asset.clone();
        helpers::position::upsert_lending_account(state, &mut delta, &pid, |p| {
            if let PositionKind::LendingAccount(la) = &mut p.kind {
                // `reduce_collateral` returns an `Err` if the asset is
                // missing or amount exceeds the held collateral, but the
                // closure must be infallible. We accept that and surface
                // the precise reason from the helper below.
                let _ = super::reduce_collateral(la, &asset, intended_amount);
            }
        })?;
        // Re-run the reduction against a separate clone for the strict
        // accounting — the upsert above is the public state mutation, this
        // call is the authoritative validation.
        let pos = state
            .positions
            .iter()
            .find(|p| p.id == pid)
            .ok_or_else(|| ReducerError::PositionNotFound(pid.clone()))?;
        if let PositionKind::LendingAccount(la) = &pos.kind {
            let mut tmp = la.clone();
            super::reduce_collateral(&mut tmp, &self.asset, intended_amount)?;
        }

        // Step 5 — credit the underlying back to the wallet. The action
        // type carries `recipient` separately from the wallet owner — when
        // a third party is paid the wallet's own balance is unaffected; we
        // only credit when the recipient is the wallet owner.
        if self.recipient == state.wallet_id.address {
            helpers::balance::credit(state, &mut delta, &self.asset.key, intended_amount)?;
        }

        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::{
        LendingVenue, ReserveState, UserLendingState, WithdrawLiveInputs,
    };
    use simulation_state::delta::TokenChange;
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::position::{LendingAccount, Position, PositionKind};
    use simulation_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, Time, VenueRef,
    };
    use simulation_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
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

    fn make_holding(amount: u128) -> TokenHolding {
        TokenHolding {
            key: usdc_ref().key,
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(amount)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn aave_v3_venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    fn lending_position(collat: u128) -> Position {
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
                collaterals: vec![(usdc_ref(), U256::from(collat))],
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

    fn state_with_position(balance: u128, collat: u128) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(balance));
        s.positions.push(lending_position(collat));
        s
    }

    fn reserve_with(paused: bool) -> ReserveState {
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
            is_paused: paused,
        }
    }

    fn withdraw_action(amount: U256, paused: bool, available: u128) -> WithdrawAction {
        WithdrawAction {
            venue: aave_v3_venue(),
            asset: usdc_ref(),
            amount,
            recipient: user(),
            live_inputs: WithdrawLiveInputs {
                reserve_state: LiveField::new(
                    reserve_with(paused),
                    DataSource::UserSupplied,
                    now(),
                ),
                available_to_withdraw: LiveField::new(
                    U256::from(available),
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

    /// Happy path: withdraw `1_000` against `5_000` collateral. Emits an
    /// `Update` (collateral reduced) and a credit (recipient = wallet owner).
    #[test]
    fn withdraw_happy_path_partial() {
        let state = state_with_position(0, 5_000);
        let action = withdraw_action(U256::from(1_000u64), false, 5_000);
        let delta = action.apply(&state, &ctx()).unwrap();

        assert_eq!(delta.position_changes.len(), 1);
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));

        // Credit lands as a positive BalanceDelta on USDC.
        let credit = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if *key == usdc_ref().key && delta.is_positive() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected USDC credit");
        assert_eq!(credit, "1000");
    }

    /// Max-withdraw (`amount = U256::MAX`) uses the `LiveField` as the
    /// intended amount.
    #[test]
    fn withdraw_max_uses_live_field_available() {
        let state = state_with_position(0, 5_000);
        let action = withdraw_action(U256::MAX, false, 5_000);
        let delta = action.apply(&state, &ctx()).unwrap();

        let credit = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if *key == usdc_ref().key && delta.is_positive() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected USDC credit");
        assert_eq!(credit, "5000");
    }

    /// Withdraw against a paused reserve is rejected.
    #[test]
    fn withdraw_paused_reserve_is_invariant_error() {
        let state = state_with_position(0, 5_000);
        let action = withdraw_action(U256::from(1_000u64), true, 5_000);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("paused")));
    }

    /// Withdraw without a prior supply → `PositionNotFound`.
    #[test]
    fn withdraw_no_position_returns_position_not_found() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(0));
        let action = withdraw_action(U256::from(1_000u64), false, 1_000);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }

    /// Withdraw exceeding collateral → `Invariant`.
    #[test]
    fn withdraw_amount_exceeding_collateral_is_invariant() {
        let state = state_with_position(0, 1_000);
        let action = withdraw_action(U256::from(5_000u64), false, 5_000);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("amount")));
    }
}
