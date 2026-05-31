//! `RepayAction` reducer — repay an outstanding debt position.
//!
//! Flow (PDF §6.4):
//!
//! 1. Validate `live_inputs.reserve_state` — reject paused (frozen reserves
//!    still permit repays).
//! 2. Look up the `LendingAccount` — `PositionNotFound` when missing.
//! 3. `amount == U256::MAX` → full repay; substitute `live_inputs.current_debt`.
//! 4. `use_a_tokens = true` (Aave V3 option) repays directly via the
//!    wallet's `aToken` balance — under the Phase 2 1:1 approximation we
//!    still debit the underlying. The flag is recorded for downstream
//!    auditing but does not change the helper sequence; a follow-up commit
//!    will route to a distinct aToken `TokenKey` once the sync orchestrator
//!    surfaces those holdings.
//! 5. Reduce the `LendingAccount.debts` entry via `reduce_debt`.
//! 6. `balance::debit` the repaid asset from the wallet (HF is auto-improved
//!    by the debt reduction; no inline HF check required).

use simulation_state::position::PositionKind;
use simulation_state::primitives::U256;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::RepayAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::position_id;

impl Reducer for RepayAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;
        let reserve = &self.live_inputs.reserve_state.value;
        let venue_tag = super::venue_tag(&self.venue);

        if reserve.is_paused {
            return Err(ReducerError::Invariant(format!(
                "repay rejected: reserve paused for venue {venue_tag}"
            )));
        }

        let pid = position_id::for_venue(&self.venue);
        if !super::position_exists(state, &StateDelta::new(), &pid) {
            return Err(ReducerError::PositionNotFound(pid));
        }

        let intended_amount = if self.amount == U256::MAX {
            self.live_inputs.current_debt.value
        } else {
            self.amount
        };

        let mut delta = StateDelta::new();

        let asset = self.asset.clone();
        let rate_mode = self.rate_mode.clone();
        helpers::position::upsert_lending_account(state, &mut delta, &pid, |p| {
            if let PositionKind::LendingAccount(la) = &mut p.kind {
                let _ = super::reduce_debt(la, &asset, intended_amount, &rate_mode);
            }
        })?;

        // Authoritative strict check (the closure swallowed the result).
        let pos = state
            .positions
            .iter()
            .find(|p| p.id == pid)
            .ok_or_else(|| ReducerError::PositionNotFound(pid.clone()))?;
        if let PositionKind::LendingAccount(la) = &pos.kind {
            let mut tmp = la.clone();
            super::reduce_debt(&mut tmp, &self.asset, intended_amount, &self.rate_mode)?;
        }

        helpers::balance::debit(state, &mut delta, &self.asset.key, intended_amount)?;

        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::{LendingVenue, RepayLiveInputs, ReserveState, UserLendingState};
    use simulation_state::delta::TokenChange;
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::position::{LendingAccount, Position, PositionKind};
    use simulation_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, Time, VenueRef,
    };
    use simulation_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, RateMode, TokenHolding, TokenKey,
        TokenKind, TokenRef,
    };
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

    fn lending_position_with_debt(debt: u128) -> Position {
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
                collaterals: vec![(usdc_ref(), U256::from(10_000u64))],
                debts: vec![(usdc_ref(), U256::from(debt), RateMode::Variable)],
                emode: None,
                is_isolated: false,
                health_factor: LiveField::new(Decimal::new("2.0"), DataSource::UserSupplied, now()),
                ltv: LiveField::new(Decimal::new("0.5"), DataSource::UserSupplied, now()),
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

    fn state_with(balance: u128, debt: u128) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(balance));
        s.positions.push(lending_position_with_debt(debt));
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

    fn repay_action(amount: U256, paused: bool, current_debt: u128) -> RepayAction {
        RepayAction {
            venue: aave_v3_venue(),
            asset: usdc_ref(),
            amount,
            rate_mode: RateMode::Variable,
            on_behalf_of: None,
            use_a_tokens: false,
            live_inputs: RepayLiveInputs {
                reserve_state: LiveField::new(
                    reserve_with(paused),
                    DataSource::UserSupplied,
                    now(),
                ),
                current_debt: LiveField::new(
                    U256::from(current_debt),
                    DataSource::UserSupplied,
                    now(),
                ),
                user_state_before: LiveField::new(
                    UserLendingState {
                        health_factor: Decimal::new("2.0"),
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

    /// Happy path: repay `500` against `1_000` debt.
    #[test]
    fn repay_happy_path_partial() {
        let state = state_with(5_000, 1_000);
        let action = repay_action(U256::from(500u64), false, 1_000);
        let delta = action.apply(&state, &ctx()).unwrap();

        let debit = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if *key == usdc_ref().key && delta.is_negative() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected USDC debit");
        assert_eq!(debit, "500");
    }

    /// Full repay via `U256::MAX` uses `current_debt` `LiveField`.
    #[test]
    fn repay_max_uses_current_debt_live_field() {
        let state = state_with(5_000, 1_000);
        let action = repay_action(U256::MAX, false, 1_000);
        let delta = action.apply(&state, &ctx()).unwrap();

        let debit = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if *key == usdc_ref().key && delta.is_negative() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected USDC debit");
        assert_eq!(debit, "1000");
    }

    /// Repay against paused reserve is rejected.
    #[test]
    fn repay_paused_reserve_is_invariant() {
        let state = state_with(5_000, 1_000);
        let action = repay_action(U256::from(500u64), true, 1_000);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("paused")));
    }

    /// Repay exceeding current debt is rejected.
    #[test]
    fn repay_overshoot_is_invariant() {
        let state = state_with(5_000, 1_000);
        let action = repay_action(U256::from(2_000u64), false, 1_000);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("repay")));
    }

    /// Repay without a prior debt position is `PositionNotFound`.
    #[test]
    fn repay_no_position_is_position_not_found() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(5_000));
        let action = repay_action(U256::from(500u64), false, 1_000);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }
}
