//! `SupplyAction` reducer — supply (`deposit`) an asset into a lending market.
//!
//! Flow (PDF §6.1):
//!
//! 1. Validate `live_inputs.reserve_state` — reject when frozen, paused, or
//!    the supply cap would be exceeded.
//! 2. Validate `eligible_as_collat` — if `false`, the supplied asset cannot
//!    back a borrow but is still legal to supply (the `LiveField` just gates
//!    whether the wallet's `LendingAccount.collaterals` is incremented).
//! 3. Dispatch on `self.venue` to the venue-specific math (`aave_v3`,
//!    `compound_v2`, …) to validate the reserve invariant. The Phase 2
//!    approximation treats receipt amounts as 1:1 with the underlying;
//!    receipt-token credits are emitted via the `LendingAccount.collaterals`
//!    update rather than as a separate ERC20 mint.
//! 4. `balance::debit` removes the underlying asset from the wallet.
//! 5. Open-or-update the `LendingAccount` position for this venue. The
//!    position id is the deterministic `super::position_id_for_venue` so
//!    repeated supplies merge into the same account.

use simulation_state::position::{LendingAccount, Position, PositionKind};
use simulation_state::primitives::{Decimal, MarketRef, ProtocolRef, VenueRef};
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::{LendingVenue, SupplyAction};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::{aave_v2, aave_v3, compound_v2, compound_v3, fluid, morpho_optimizer, position_id, spark};

impl Reducer for SupplyAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let reserve = &self.live_inputs.reserve_state.value;

        // Step 1 — paused / frozen / cap.
        if reserve.is_paused {
            return Err(ReducerError::Invariant(format!(
                "supply rejected: reserve paused for venue {}",
                super::venue_tag(&self.venue),
            )));
        }
        if reserve.is_frozen {
            return Err(ReducerError::Invariant(format!(
                "supply rejected: reserve frozen for venue {}",
                super::venue_tag(&self.venue),
            )));
        }
        if let Some(cap) = reserve.supply_cap {
            let projected = reserve.total_supply.saturating_add(self.amount);
            if projected > cap {
                return Err(ReducerError::Invariant(format!(
                    "supply rejected: total_supply {} + amount {} > supply_cap {}",
                    reserve.total_supply, self.amount, cap
                )));
            }
        }

        // Step 3 — venue math validation (reserve invariant). The returned
        // receipt amount is informational under the Phase 2 1:1 approximation;
        // it is recorded as the collateral delta below.
        let receipt_amount = match &self.venue {
            LendingVenue::AaveV3 { .. } => {
                aave_v3::asset_to_atokens(state, ctx, reserve, self.amount)?
            }
            LendingVenue::AaveV2 { .. } => {
                aave_v2::asset_to_atokens(state, ctx, reserve, self.amount)?
            }
            LendingVenue::Spark { .. } => {
                spark::asset_to_atokens(state, ctx, reserve, self.amount)?
            }
            LendingVenue::CompoundV2 { .. } => {
                compound_v2::underlying_to_ctoken(reserve, self.amount)?
            }
            LendingVenue::CompoundV3 { .. } => {
                compound_v3::principal_to_present_value(reserve, self.amount)?
            }
            LendingVenue::MorphoOptimizer { .. } => {
                morpho_optimizer::asset_to_optimizer_shares(reserve, self.amount)?
            }
            LendingVenue::Fluid { .. } => {
                fluid::asset_to_fluid_shares(reserve, self.amount)?
            }
            LendingVenue::MorphoBlue { .. } => {
                // Morpho Blue shares need (total_assets, total_shares) which
                // `ReserveState` does not yet expose — defer until the sync
                // orchestrator wires those values through.
                return Err(ReducerError::UnsupportedProtocol {
                    action: "supply".into(),
                    protocol: "morpho_blue".into(),
                });
            }
        };
        let _ = receipt_amount;

        let mut delta = StateDelta::new();

        // Step 4 — debit the wallet's underlying asset.
        helpers::balance::debit(state, &mut delta, &self.asset.key, self.amount)?;

        // Step 5 — open-or-update the LendingAccount position.
        let pid = position_id::for_venue(&self.venue);
        if super::position_exists(state, &delta, &pid) {
            helpers::position::upsert_lending_account(state, &mut delta, &pid, |p| {
                if let PositionKind::LendingAccount(la) = &mut p.kind {
                    super::merge_collateral(la, &self.asset, self.amount);
                }
            })?;
        } else if self.live_inputs.eligible_as_collat.value {
            // Open a fresh LendingAccount with this asset as the seed
            // collateral entry.
            let pos = new_lending_account(self, ctx);
            helpers::position::open_position(state, &mut delta, pos)?;
        } else {
            // Asset is not eligible as collateral — open a position with
            // empty collateral; the supply is still recorded via the debit
            // above (the underlying asset leaves the wallet).
            let mut pos = new_lending_account(self, ctx);
            if let PositionKind::LendingAccount(la) = &mut pos.kind {
                la.collaterals.clear();
            }
            helpers::position::open_position(state, &mut delta, pos)?;
        }

        Ok(delta)
    }
}

/// Build a fresh `LendingAccount` position for a first-supply event.
fn new_lending_account(action: &SupplyAction, ctx: &EvalContext) -> Position {
    let pid = position_id::for_venue(&action.venue);
    let venue_tag = super::venue_tag(&action.venue);
    let market = MarketRef {
        symbol: venue_tag.to_string(),
        venue: VenueRef::new(venue_tag),
    };
    let acct = LendingAccount {
        market,
        collaterals: vec![(action.asset.clone(), action.amount)],
        debts: vec![],
        emode: None,
        is_isolated: false,
        health_factor: derived_live_field(Decimal::new("999999999"), ctx),
        ltv: derived_live_field(Decimal::zero(), ctx),
        liquidation_threshold: derived_live_field(
            Decimal::new(format!("0.{:04}", action.live_inputs.reserve_state.value.liquidation_threshold_bp)),
            ctx,
        ),
    };
    Position {
        id: pid,
        protocol: ProtocolRef::new(super::venue_tag(&action.venue)),
        chain: Some(super::venue_chain(&action.venue)),
        kind: PositionKind::LendingAccount(acct),
        primitives_synced_at: ctx.now,
        primitives_source: simulation_state::live_field::DataSource::UserSupplied,
    }
}

/// Construct a derived `LiveField<Decimal>` for HF / LTV / LT slots, tagged
/// with the reducer-derived source so the sync orchestrator knows it was
/// computed (not fetched).
fn derived_live_field(value: Decimal, ctx: &EvalContext) -> simulation_state::LiveField<Decimal> {
    use simulation_state::live_field::DataSource;
    simulation_state::LiveField::new(
        value,
        DataSource::DerivedFrom {
            inputs: vec![],
            calc_id: "lending_supply_init".into(),
        },
        ctx.now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::{ReserveState, SupplyLiveInputs, UserLendingState};
    use simulation_state::delta::TokenChange;
    use simulation_state::eval_context::RequestKind;
    use simulation_state::PositionChange;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::primitives::{Address, ChainId, Price, Time, U256};
    use simulation_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
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
            last_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn state_with_balance(amount: u128) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(amount));
        s
    }

    fn aave_v3_venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    fn reserve_with(supply_cap: Option<U256>, is_paused: bool, is_frozen: bool) -> ReserveState {
        ReserveState {
            total_supply: U256::from(1_000_000u64),
            total_borrow: U256::from(500_000u64),
            utilization_bp: 5_000,
            supply_cap,
            borrow_cap: None,
            ltv_bp: 8_000,
            liquidation_threshold_bp: 8_250,
            liquidation_bonus_bp: 500,
            reserve_factor_bp: 1_000,
            is_frozen,
            is_paused,
        }
    }

    fn supply_action(
        amount: u128,
        cap: Option<U256>,
        paused: bool,
        frozen: bool,
        eligible_as_collat: bool,
    ) -> SupplyAction {
        let reserve = reserve_with(cap, paused, frozen);
        SupplyAction {
            venue: aave_v3_venue(),
            asset: usdc_ref(),
            amount: U256::from(amount),
            on_behalf_of: None,
            live_inputs: SupplyLiveInputs {
                reserve_state: LiveField::new(reserve, DataSource::UserSupplied, now()),
                supply_apy: LiveField::new(Decimal::new("0.04"), DataSource::UserSupplied, now()),
                a_token_price_usd: LiveField::new(
                    Price::from("1"),
                    DataSource::UserSupplied,
                    now(),
                ),
                eligible_as_collat: LiveField::new(
                    eligible_as_collat,
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

    /// Happy path: first supply opens a new `LendingAccount` position and
    /// emits a single `BalanceDelta` for the underlying debit.
    #[test]
    fn supply_happy_path_opens_position_and_debits() {
        let state = state_with_balance(10_000_000);
        let action = supply_action(1_000_000, None, false, false, true);
        let delta = action.apply(&state, &ctx()).unwrap();

        // One token change: the USDC debit.
        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "1000000");
            }
            other => panic!("expected BalanceDelta, got {other:?}"),
        }

        // One position change: Open with the asset seeded as collateral.
        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Open { position } => {
                assert!(position.id.starts_with("aave_v3:"));
                if let PositionKind::LendingAccount(la) = &position.kind {
                    assert_eq!(la.collaterals.len(), 1);
                    assert_eq!(la.collaterals[0].0, usdc_ref());
                    assert_eq!(la.collaterals[0].1, U256::from(1_000_000u64));
                } else {
                    panic!("expected LendingAccount");
                }
            }
            other => panic!("expected Open, got {other:?}"),
        }
    }

    /// Supply against a paused reserve must surface `Invariant` and emit no
    /// state changes.
    #[test]
    fn supply_paused_reserve_is_invariant_error() {
        let state = state_with_balance(10_000_000);
        let action = supply_action(1_000_000, None, true, false, true);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("paused")));
    }

    /// Supply that would breach the supply cap surfaces `Invariant`.
    #[test]
    fn supply_cap_breach_is_invariant_error() {
        let state = state_with_balance(10_000_000);
        // total_supply = 1_000_000; cap = 1_500_000; amount = 600_000 would push to 1_600_000.
        let action = supply_action(600_000, Some(U256::from(1_500_000u64)), false, false, true);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("supply_cap")));
    }

    /// Unsupported venue (Morpho Blue requires shares-state not yet wired)
    /// surfaces `UnsupportedProtocol`.
    #[test]
    fn supply_morpho_blue_is_unsupported() {
        let state = state_with_balance(10_000_000);
        let mut action = supply_action(1_000_000, None, false, false, true);
        action.venue = LendingVenue::MorphoBlue {
            chain: ChainId::ethereum_mainnet(),
            market_id: "0xdeadbeef".into(),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::UnsupportedProtocol { ref protocol, .. } if protocol == "morpho_blue"));
    }
}
