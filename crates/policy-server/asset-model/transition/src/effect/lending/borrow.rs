//! `BorrowAction` reducer — borrow an asset against existing collateral.
//! Flow (PDF §6.3):
//! 1. Validate `live_inputs.reserve_state` — reject paused / frozen.
//!    Frozen reserves block new borrows on Aave V3 (unlike withdraws).
//! 2. Reject when `available_liquidity < amount` (the pool doesn't have
//!    enough free cash to fund the borrow).
//! 3. Reject when `borrow_cap` would be breached.
//! 4. Look up the `LendingAccount` for this venue — `PositionNotFound`
//!    when no prior supply exists (you can't borrow without collateral).
//! 5. Merge the borrow into `LendingAccount.debts` and recompute HF.
//! 6. Reject when post-borrow `HF < 1` (the borrow would liquidate the
//!    account on entry).
//! 7. `balance::credit` the borrowed asset to the wallet.

use policy_state::position::PositionKind;
use policy_state::primitives::Decimal;
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::BorrowAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::position_id;

impl Reducer for BorrowAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let reserve = &self.live_inputs.reserve_state.value;
        let venue_tag = super::venue_tag(&self.venue);

        if reserve.is_paused {
            return Err(ReducerError::Invariant(format!(
                "borrow rejected: reserve paused for venue {venue_tag}"
            )));
        }
        if reserve.is_frozen {
            return Err(ReducerError::Invariant(format!(
                "borrow rejected: reserve frozen for venue {venue_tag}"
            )));
        }

        if self.live_inputs.available_liquidity.value < self.amount {
            return Err(ReducerError::Invariant(format!(
                "borrow rejected: available_liquidity {} < amount {}",
                self.live_inputs.available_liquidity.value, self.amount,
            )));
        }

        if let Some(cap) = reserve.borrow_cap {
            let projected = reserve.total_borrow.saturating_add(self.amount);
            if projected > cap {
                return Err(ReducerError::Invariant(format!(
                    "borrow rejected: total_borrow {} + amount {} > borrow_cap {}",
                    reserve.total_borrow, self.amount, cap
                )));
            }
        }

        let pid = position_id::for_venue(&self.venue);
        if !super::position_exists(state, &StateDelta::new(), &pid) {
            return Err(ReducerError::PositionNotFound(pid));
        }

        let mut delta = StateDelta::new();

        // Step 5 — record the borrow in the LendingAccount debts list.
        let asset = self.asset.clone();
        let amount = self.amount;
        let rate_mode = self.rate_mode.clone();
        helpers::position::upsert_lending_account(state, &mut delta, &pid, |p| {
            if let PositionKind::LendingAccount(la) = &mut p.kind {
                super::merge_debt(la, &asset, amount, &rate_mode);
            }
        })?;

        // Step 6 — HF recompute against the post-borrow state. The
        // existing on-chain `LendingAccount` does not yet hold the new
        // debt; we clone + apply the same merge for the HF preview.
        let existing_pos = state
            .positions
            .iter()
            .find(|p| p.id == pid)
            .ok_or_else(|| ReducerError::PositionNotFound(pid.clone()))?;
        let PositionKind::LendingAccount(la) = &existing_pos.kind else {
            return Err(ReducerError::Invariant(format!(
                "borrow: position {pid} is not a LendingAccount"
            )));
        };
        let mut preview = la.clone();
        super::merge_debt(&mut preview, &self.asset, self.amount, &self.rate_mode);

        let (collat_prices, debt_prices, lts) = super::build_price_tables(
            &preview,
            &self.live_inputs.asset_price_usd.value,
            &self.asset,
        );
        let hf = helpers::derived::recompute_health_factor(
            &preview,
            &collat_prices,
            &debt_prices,
            &lts,
            ctx.now,
        )?;
        if !hf_is_safe(&hf)? {
            return Err(ReducerError::Invariant(format!(
                "borrow rejected: post-borrow HF {hf:?} < 1.0"
            )));
        }

        // Step 7 — credit the borrowed asset.
        helpers::balance::credit(state, &mut delta, &self.asset.key, self.amount)?;

        Ok(delta)
    }
}

/// `true` when the recomputed HF is >= 1. Sentinel `HF_INFINITY`
/// (`999999999`) also passes since it represents zero debt.
fn hf_is_safe(hf: &Decimal) -> ReducerResult<bool> {
    use std::str::FromStr;
    let parsed = rust_decimal::Decimal::from_str(hf.as_str())
        .map_err(|e| ReducerError::Invariant(format!("borrow: HF Decimal parse failed: {e}")))?;
    Ok(parsed >= rust_decimal::Decimal::from(1u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::{BorrowLiveInputs, LendingVenue, ReserveState, UserLendingState};
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::position::{LendingAccount, Position, PositionKind};
    use policy_state::primitives::{
        Address, ChainId, MarketRef, Price, ProtocolRef, Time, VenueRef, U256,
    };
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, RateMode, TokenHolding, TokenKey,
        TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
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

    fn state_with(balance: u128, collat: u128) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(balance));
        s.positions.push(lending_position(collat));
        s
    }

    fn reserve_with(paused: bool, frozen: bool, borrow_cap: Option<U256>) -> ReserveState {
        ReserveState {
            total_supply: U256::from(1_000_000u64),
            total_borrow: U256::from(500_000u64),
            utilization_bp: 5_000,
            supply_cap: None,
            borrow_cap,
            ltv_bp: 8_000,
            liquidation_threshold_bp: 8_250,
            liquidation_bonus_bp: 500,
            reserve_factor_bp: 1_000,
            is_frozen: frozen,
            is_paused: paused,
        }
    }

    fn borrow_action(
        amount: u128,
        paused: bool,
        frozen: bool,
        borrow_cap: Option<U256>,
        available_liquidity: u128,
        price: &str,
    ) -> BorrowAction {
        BorrowAction {
            venue: aave_v3_venue(),
            asset: usdc_ref(),
            amount: U256::from(amount),
            rate_mode: RateMode::Variable,
            on_behalf_of: None,
            live_inputs: BorrowLiveInputs {
                reserve_state: LiveField::new(
                    reserve_with(paused, frozen, borrow_cap),
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
                asset_price_usd: LiveField::new(
                    Price::from(price),
                    DataSource::UserSupplied,
                    now(),
                ),
                current_borrow_rate: LiveField::new(
                    Decimal::new("0.04"),
                    DataSource::UserSupplied,
                    now(),
                ),
                available_liquidity: LiveField::new(
                    U256::from(available_liquidity),
                    DataSource::UserSupplied,
                    now(),
                ),
            },
        }
    }

    /// Happy path: borrow `1_000` USDC against `5_000` USDC collateral.
    /// `HF = (5_000 * 1 * 0.825) / (1_000 * 1) = 4.125 > 1` → passes.
    #[test]
    fn borrow_happy_path_safe_hf() {
        let state = state_with(0, 5_000);
        let action = borrow_action(1_000, false, false, None, 1_000_000, "1");
        let delta = action.apply(&state, &ctx()).unwrap();

        // Credit the borrowed asset.
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

    /// Borrow that would push HF < 1 must be rejected.
    /// `5_000 USDC * 1 * 0.825 = 4_125` borrow value at 1$ → borrow of
    /// `5_000` → `HF = 4_125 / 5_000 = 0.825 < 1`.
    #[test]
    fn borrow_unsafe_hf_is_rejected() {
        let state = state_with(0, 5_000);
        let action = borrow_action(5_000, false, false, None, 1_000_000, "1");
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("HF")));
    }

    /// Borrow against insufficient pool liquidity → Invariant.
    #[test]
    fn borrow_insufficient_liquidity_is_invariant() {
        let state = state_with(0, 5_000);
        let action = borrow_action(2_000, false, false, None, 500, "1");
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("available_liquidity")));
    }

    /// Borrow cap breach is rejected.
    #[test]
    fn borrow_cap_breach_is_invariant() {
        let state = state_with(0, 5_000);
        // total_borrow = 500_000; cap = 600_000; amount = 200_000 → 700_000.
        let action = borrow_action(
            200_000,
            false,
            false,
            Some(U256::from(600_000u64)),
            1_000_000,
            "1",
        );
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("borrow_cap")));
    }

    /// Borrow against a paused / frozen reserve is rejected.
    #[test]
    fn borrow_paused_reserve_is_invariant() {
        let state = state_with(0, 5_000);
        let action = borrow_action(1_000, true, false, None, 1_000_000, "1");
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("paused")));
    }

    /// Borrow without a prior supply (no `LendingAccount`) → `PositionNotFound`.
    #[test]
    fn borrow_no_position_is_position_not_found() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(0));
        let action = borrow_action(1_000, false, false, None, 1_000_000, "1");
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }
}
