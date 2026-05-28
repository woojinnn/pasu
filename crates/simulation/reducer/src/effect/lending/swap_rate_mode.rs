//! `SwapRateModeAction` reducer — `Aave` switch between `Variable` and `Stable`
//! debt.
//!
//! Aave-only. Other venues do not support stable-rate debt and surface
//! `UnsupportedProtocol`.
//!
//! Flow (PDF §6.9):
//!
//! 1. Reject venues other than Aave V2 / V3.
//! 2. Look up the `LendingAccount` — `PositionNotFound` when missing.
//! 3. Move the debt from the *other* rate mode to `new_mode`:
//!    - Find the existing `(asset, !new_mode)` entry; surface `Invariant`
//!      when missing (nothing to swap).
//!    - Drop the source entry; merge into the destination entry.
//! 4. No balance change (token balances are unchanged; only the rate-mode
//!    flag on the debt is flipped).

use simulation_state::position::PositionKind;
use simulation_state::token::RateMode;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::{LendingVenue, SwapRateModeAction};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::position_id;

impl Reducer for SwapRateModeAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;
        if !matches!(self.venue, LendingVenue::AaveV2 { .. } | LendingVenue::AaveV3 { .. }) {
            return Err(ReducerError::UnsupportedProtocol {
                action: "swap_rate_mode".into(),
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
                "swap_rate_mode: position {pid} is not a LendingAccount"
            )));
        };

        let source_mode = other_mode(&self.new_mode);
        let (source_amount, source_asset) = la
            .debts
            .iter()
            .find(|(t, _, r)| t == &self.asset && r == &source_mode)
            .map(|(t, amt, _)| (*amt, t.clone()))
            .ok_or_else(|| {
                ReducerError::Invariant(format!(
                    "swap_rate_mode: no debt entry for asset {:?} in mode {source_mode:?}",
                    self.asset.key
                ))
            })?;

        let mut delta = StateDelta::new();
        let new_mode = self.new_mode.clone();
        helpers::position::upsert_lending_account(state, &mut delta, &pid, |p| {
            if let PositionKind::LendingAccount(la) = &mut p.kind {
                let _ = super::reduce_debt(la, &source_asset, source_amount, &source_mode);
                super::merge_debt(la, &source_asset, source_amount, &new_mode);
            }
        })?;

        Ok(delta)
    }
}

/// Swap `Variable` ↔ `Stable`. `Fixed` is treated as `Variable` for the
/// purposes of swap-rate-mode (Aave does not actually expose `Fixed` debt
/// — defensive default).
const fn other_mode(mode: &RateMode) -> RateMode {
    match mode {
        RateMode::Variable | RateMode::Fixed => RateMode::Stable,
        RateMode::Stable => RateMode::Variable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::SwapRateModeLiveInputs;
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::position::{LendingAccount, Position, PositionKind};
    use simulation_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, Time, VenueRef, U256,
    };
    use simulation_state::token::{RateMode, TokenKey, TokenRef};
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

    fn aave_v3_venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    fn lending_with_debt(asset: TokenRef, amount: u128, mode: RateMode) -> Position {
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
                collaterals: vec![],
                debts: vec![(asset, U256::from(amount), mode)],
                emode: None,
                is_isolated: false,
                health_factor: LiveField::new(
                    Decimal::new("2.0"),
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

    fn state_with(asset: TokenRef, amount: u128, mode: RateMode) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions.push(lending_with_debt(asset, amount, mode));
        s
    }

    fn action(new_mode: RateMode) -> SwapRateModeAction {
        SwapRateModeAction {
            venue: aave_v3_venue(),
            asset: usdc_ref(),
            new_mode,
            live_inputs: SwapRateModeLiveInputs {
                current_debts: LiveField::new(
                    (U256::from(1_000u64), U256::ZERO),
                    DataSource::UserSupplied,
                    now(),
                ),
                rates: LiveField::new(
                    (Decimal::new("0.04"), Decimal::new("0.05")),
                    DataSource::UserSupplied,
                    now(),
                ),
            },
        }
    }

    /// Happy path: Variable → Stable. Source entry dropped, dest created.
    #[test]
    fn swap_variable_to_stable_happy_path() {
        let state = state_with(usdc_ref(), 1_000, RateMode::Variable);
        let delta = action(RateMode::Stable).apply(&state, &ctx()).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
        // No token changes.
        assert!(delta.token_changes.is_empty());
    }

    /// Swap target mode with no existing source entry is `Invariant`.
    #[test]
    fn swap_no_source_entry_is_invariant() {
        let state = state_with(usdc_ref(), 1_000, RateMode::Variable);
        // Already in Variable; cannot swap "to Variable" because there is no
        // Stable entry to swap from.
        let err = action(RateMode::Variable).apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("no debt entry")));
    }

    /// Non-Aave venue: unsupported.
    #[test]
    fn swap_non_aave_unsupported() {
        let state = state_with(usdc_ref(), 1_000, RateMode::Variable);
        let mut a = action(RateMode::Stable);
        a.venue = LendingVenue::CompoundV2 {
            chain: ChainId::ethereum_mainnet(),
            comptroller: Address::from_str("0x3d9819210a31b4961b30ef54be2aed79b9c9cd3b").unwrap(),
        };
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::UnsupportedProtocol { .. }));
    }
}
