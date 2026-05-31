//! `hl_withdraw` reducer — record a USDC withdrawal (decision B: always record
//! the outflow intent; also decrement `perp_usdc` when a base balance exists).

use simulation_state::position::{HlAccount, PositionKind};
use simulation_state::primitives::Decimal;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlWithdrawAction;
use crate::error::ReducerResult;
use crate::helpers;

use super::common;

pub(super) fn apply(
    action: &HlWithdrawAction,
    state: &WalletState,
    ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    apply_outflow(&action.amount, state, ctx)
}

/// Shared withdraw / `usd_send` body: both are USDC outflows from the perp account.
pub(super) fn apply_outflow(
    amount: &Decimal,
    state: &WalletState,
    ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    let mut delta = StateDelta::new();

    if let Some(base) = common::find_hl_account(state) {
        // Compute absolute results up front so an underflow errors before we
        // emit any change.
        let new_usdc = common::decimal_sub_nonneg(&base.perp_usdc, amount)?;
        let new_outflow = common::decimal_add(&base.pending_outflow, amount)?;
        let id = common::HL_ACCOUNT_ID.to_owned();
        helpers::position::upsert_hl_account(state, &mut delta, &id, |pos| {
            if let PositionKind::HyperliquidAccount(a) = &mut pos.kind {
                a.perp_usdc = new_usdc.clone();
                a.pending_outflow = new_outflow.clone();
            }
        })?;
    } else {
        let acct = HlAccount {
            perp_usdc: Decimal::new("0"),
            pending_outflow: amount.clone(),
            ..HlAccount::default()
        };
        helpers::position::open_position(state, &mut delta, common::hl_position(acct, ctx.now))?;
    }
    Ok(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    use simulation_state::eval_context::RequestKind;
    use simulation_state::position::{HlAccount, Position, PositionKind};
    use simulation_state::primitives::{Address, ChainId, Decimal, Time};
    use simulation_state::wallet::{WalletId, WalletState};
    use simulation_state::PositionChange;

    use super::super::common::HL_ACCOUNT_ID;

    fn ctx() -> EvalContext {
        EvalContext::new(
            ChainId::ethereum_mainnet(),
            Time::from_unix(1),
            RequestKind::Signature,
        )
    }
    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from([0u8; 20]),
            std::iter::empty::<ChainId>(),
        ))
    }
    fn act(amount: &str) -> HlWithdrawAction {
        HlWithdrawAction {
            destination: Address::from([0xde; 20]),
            amount: Decimal::new(amount),
        }
    }
    fn state_with_usdc(bal: &str) -> WalletState {
        let mut s = empty_state();
        s.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: super::super::common::hl_protocol_ref(),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_usdc: Decimal::new(bal),
                pending_outflow: Decimal::new("0"),
                ..HlAccount::default()
            }),
            primitives_synced_at: Time::from_unix(1),
            primitives_source: simulation_state::live_field::DataSource::UserSupplied,
        });
        s
    }
    fn hl_of(state: &WalletState) -> HlAccount {
        state
            .positions
            .iter()
            .find_map(|p| match &p.kind {
                PositionKind::HyperliquidAccount(a) => Some(a.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn withdraw_on_empty_base_records_outflow_intent() {
        let delta = apply(&act("1000.5"), &empty_state(), &ctx()).unwrap();
        match &delta.position_changes[0] {
            PositionChange::Open { position } => match &position.kind {
                PositionKind::HyperliquidAccount(a) => {
                    assert_eq!(a.pending_outflow, Decimal::new("1000.5"));
                    assert_eq!(a.perp_usdc, Decimal::new("0"));
                }
                o => panic!("{o:?}"),
            },
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn withdraw_on_existing_base_decrements_usdc_and_adds_outflow() {
        let delta = apply(&act("400"), &state_with_usdc("1000"), &ctx()).unwrap();
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));
        let next = crate::helpers::delta::apply_delta(&state_with_usdc("1000"), &delta).unwrap();
        let a = hl_of(&next);
        assert_eq!(a.perp_usdc, Decimal::new("600"));
        assert_eq!(a.pending_outflow, Decimal::new("400"));
    }

    #[test]
    fn withdraw_exceeding_base_is_invariant_error() {
        let err = apply(&act("2000"), &state_with_usdc("1000"), &ctx()).unwrap_err();
        assert!(matches!(err, crate::error::ReducerError::Invariant(_)));
    }
}
