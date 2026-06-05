//! `hl_withdraw` reducer — record a USDC withdrawal (decision B: always record
//! the outflow intent; also decrement `perp_usdc` only when a real (synced)
//! balance is present (`Some`). An unsynced (`None`) account records intent only.

use policy_state::position::{HlAccount, PositionKind};
use policy_state::primitives::Decimal;
use policy_state::{EvalContext, StateDelta, WalletState};

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
    // A negative outflow is a malformed intent; reject it (fail-closed) and
    // normalize so both branches store a canonical amount.
    let amount = common::decimal_nonneg(amount)?;

    let mut delta = StateDelta::new();

    if let Some(base) = common::find_hl_account(state) {
        // perp_usdc is guarded only when it is a real (synced) balance; an
        // unsynced account (`None`) records the outflow intent without a balance
        // check — there is no balance to validate against (fail-OPEN on intent;
        // the policy layer still gates the action). The decision-B underflow
        // guard is preserved for `Some` (real) balances.
        let new_usdc = match &base.perp_usdc {
            Some(bal) => Some(common::decimal_sub_nonneg(bal, &amount)?),
            None => None,
        };
        let new_outflow = common::decimal_add(&base.pending_outflow, &amount)?;
        let id = common::HL_ACCOUNT_ID.to_owned();
        helpers::position::upsert_hl_account(state, &mut delta, &id, |pos| {
            if let PositionKind::HyperliquidAccount(a) = &mut pos.kind {
                a.perp_usdc.clone_from(&new_usdc);
                a.pending_outflow = new_outflow.clone();
            }
        })?;
    } else {
        let acct = HlAccount {
            perp_usdc: None, // the reducer never has a synced balance
            pending_outflow: amount,
            ..HlAccount::default()
        };
        helpers::position::open_position(state, &mut delta, common::hl_position(acct, ctx.now))?;
    }
    Ok(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    use policy_state::eval_context::RequestKind;
    use policy_state::position::{HlAccount, Position, PositionKind};
    use policy_state::primitives::{Address, ChainId, Decimal, Time};
    use policy_state::wallet::{WalletId, WalletState};
    use policy_state::PositionChange;

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
    fn state_with(bal: &str, pending: &str) -> WalletState {
        // `bal` is a KNOWN / synced balance (`Some`).
        let mut s = empty_state();
        s.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: super::super::common::hl_protocol_ref(),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_usdc: Some(Decimal::new(bal)),
                pending_outflow: Decimal::new(pending),
                ..HlAccount::default()
            }),
            primitives_synced_at: Time::from_unix(1),
            primitives_source: policy_state::live_field::DataSource::UserSupplied,
        });
        s
    }
    // An account that exists but whose balance was never synced (`perp_usdc`
    // None) — exactly what the reducer produces when an earlier HL action opened
    // it.
    fn state_unsynced(pending: &str) -> WalletState {
        let mut s = empty_state();
        s.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: super::super::common::hl_protocol_ref(),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_usdc: None,
                pending_outflow: Decimal::new(pending),
                ..HlAccount::default()
            }),
            primitives_synced_at: Time::from_unix(1),
            primitives_source: policy_state::live_field::DataSource::UserSupplied,
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
                    assert_eq!(a.perp_usdc, None);
                }
                o => panic!("{o:?}"),
            },
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn withdraw_on_existing_base_decrements_usdc_and_adds_outflow() {
        let delta = apply(&act("400"), &state_with("1000", "0"), &ctx()).unwrap();
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));
        let next = crate::helpers::delta::apply_delta(&state_with("1000", "0"), &delta).unwrap();
        let a = hl_of(&next);
        assert_eq!(a.perp_usdc, Some(Decimal::new("600")));
        assert_eq!(a.pending_outflow, Decimal::new("400"));
    }

    #[test]
    fn withdraw_exceeding_base_is_invariant_error() {
        let err = apply(&act("2000"), &state_with("1000", "0"), &ctx()).unwrap_err();
        assert!(matches!(err, crate::error::ReducerError::Invariant(_)));
    }

    #[test]
    fn withdraw_accumulates_into_existing_outflow() {
        // base already has 100 pending; withdrawing 400 → 500 cumulative.
        let delta = apply(&act("400"), &state_with("1000", "100"), &ctx()).unwrap();
        let next = crate::helpers::delta::apply_delta(&state_with("1000", "100"), &delta).unwrap();
        let a = hl_of(&next);
        assert_eq!(a.perp_usdc, Some(Decimal::new("600")));
        assert_eq!(a.pending_outflow, Decimal::new("500")); // 100 + 400, not replaced
    }

    #[test]
    fn withdraw_full_balance_succeeds() {
        let delta = apply(&act("1000"), &state_with("1000", "0"), &ctx()).unwrap();
        let next = crate::helpers::delta::apply_delta(&state_with("1000", "0"), &delta).unwrap();
        assert_eq!(hl_of(&next).perp_usdc, Some(Decimal::new("0"))); // exact-zero boundary OK
    }

    #[test]
    fn withdraw_negative_amount_is_invariant_error() {
        let err = apply(&act("-500"), &state_with("1000", "0"), &ctx()).unwrap_err();
        assert!(matches!(err, crate::error::ReducerError::Invariant(_)));
    }

    #[test]
    fn withdraw_on_unsynced_existing_base_records_intent_without_error() {
        // The bug scenario: an account exists (opened by a prior HL action) but
        // perp_usdc is None. A withdraw must NOT error; it records the outflow
        // intent and leaves perp_usdc None.
        let delta = apply(&act("400"), &state_unsynced("0"), &ctx()).unwrap();
        let next = crate::helpers::delta::apply_delta(&state_unsynced("0"), &delta).unwrap();
        let a = hl_of(&next);
        assert_eq!(a.perp_usdc, None); // still unknown
        assert_eq!(a.pending_outflow, Decimal::new("400")); // intent recorded
    }

    #[test]
    fn withdraw_on_unsynced_accumulates_outflow() {
        let delta = apply(&act("400"), &state_unsynced("100"), &ctx()).unwrap();
        let next = crate::helpers::delta::apply_delta(&state_unsynced("100"), &delta).unwrap();
        assert_eq!(hl_of(&next).pending_outflow, Decimal::new("500"));
    }

    #[test]
    fn multicall_order_then_withdraw_on_empty_base_succeeds() {
        use crate::action::hyperliquid_core::{HlOrderAction, HyperliquidCoreAction};
        use crate::action::ActionBody;
        use crate::apply::Reducer;
        // An order opens the HlAccount (perp_usdc None); a following withdraw in
        // the same bundle must NOT false-underflow against the placeholder.
        let order = ActionBody::HyperliquidCore(HyperliquidCoreAction::Order(HlOrderAction {
            asset_index: 0,
            symbol: Some("BTC".to_owned()),
            is_buy: true,
            price: Decimal::new("60000"),
            size: Decimal::new("0.1"),
            reduce_only: false,
            tif: "gtc".to_owned(),
        }));
        let withdraw =
            ActionBody::HyperliquidCore(HyperliquidCoreAction::Withdraw(HlWithdrawAction {
                destination: Address::from([0xde; 20]),
                amount: Decimal::new("50"),
            }));
        let body = ActionBody::Multicall {
            actions: vec![order, withdraw],
        };
        // Must not error (previously: Invariant underflow against placeholder 0).
        let delta = body.apply(&empty_state(), &ctx()).unwrap();
        let next = crate::helpers::delta::apply_delta(&empty_state(), &delta).unwrap();
        let a = hl_of(&next);
        assert_eq!(a.perp_usdc, None); // never synced
        assert_eq!(a.open_orders.len(), 1); // order recorded
        assert_eq!(a.pending_outflow, Decimal::new("50")); // withdraw intent recorded
    }
}
