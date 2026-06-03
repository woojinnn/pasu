//! `hl_order` reducer — record an unfilled open-order intent.

use policy_state::position::{HlAccount, HlOpenOrder, PositionKind};
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlOrderAction;
use crate::error::ReducerResult;
use crate::helpers;

use super::common;

pub(super) fn apply(
    action: &HlOrderAction,
    state: &WalletState,
    ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    let order = HlOpenOrder {
        asset_index: action.asset_index,
        symbol: action.symbol.clone(),
        is_buy: action.is_buy,
        price: action.price.clone(),
        size: action.size.clone(),
        reduce_only: action.reduce_only,
        tif: action.tif.clone(),
        oid: None,
        order_type: None,
        is_trigger: None,
        trigger_price: None,
        trigger_condition: None,
        is_position_tpsl: None,
        dex: None,
    };

    let mut delta = StateDelta::new();

    if common::find_hl_account(state).is_some() {
        let id = common::HL_ACCOUNT_ID.to_owned();
        helpers::position::upsert_hl_account(state, &mut delta, &id, |pos| {
            if let PositionKind::HyperliquidAccount(a) = &mut pos.kind {
                a.open_orders.push(order.clone());
            }
        })?;
    } else {
        let acct = HlAccount {
            open_orders: vec![order],
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
            Time::from_unix(1_738_000_000),
            RequestKind::Signature,
        )
    }

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from([0u8; 20]),
            std::iter::empty::<ChainId>(),
        ))
    }

    fn order(is_buy: bool) -> HlOrderAction {
        HlOrderAction {
            asset_index: 0,
            symbol: Some("BTC".to_owned()),
            is_buy,
            price: Decimal::new("60000"),
            size: Decimal::new("0.1"),
            reduce_only: false,
            tif: "gtc".to_owned(),
        }
    }

    fn state_with_hl(acct: HlAccount) -> WalletState {
        let mut s = empty_state();
        s.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: super::super::common::hl_protocol_ref(),
            chain: None,
            kind: PositionKind::HyperliquidAccount(acct),
            primitives_synced_at: Time::from_unix(1),
            primitives_source: policy_state::live_field::DataSource::UserSupplied,
        });
        s
    }

    #[test]
    fn order_on_empty_base_opens_hl_account() {
        let delta = apply(&order(true), &empty_state(), &ctx()).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Open { position } => match &position.kind {
                PositionKind::HyperliquidAccount(a) => {
                    assert_eq!(a.open_orders.len(), 1);
                    assert_eq!(a.open_orders[0].price, Decimal::new("60000"));
                    assert_eq!(a.open_orders[0].size, Decimal::new("0.1"));
                    assert!(a.open_orders[0].is_buy);
                }
                other => panic!("expected HyperliquidAccount, got {other:?}"),
            },
            other => panic!("expected Open, got {other:?}"),
        }
    }

    #[test]
    fn order_on_existing_base_appends_open_order() {
        let base = HlAccount {
            perp_usdc: Some(Decimal::new("500")),
            open_orders: vec![],
            ..HlAccount::default()
        };
        let delta = apply(&order(false), &state_with_hl(base.clone()), &ctx()).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));

        // The Update patch carries the full snapshot; verify by applying it.
        let next = crate::helpers::delta::apply_delta(&state_with_hl(base), &delta).unwrap();
        let acct = next
            .positions
            .iter()
            .find_map(|p| match &p.kind {
                PositionKind::HyperliquidAccount(a) => Some(a.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(acct.open_orders.len(), 1); // append landed
        assert_eq!(acct.open_orders[0].price, Decimal::new("60000"));
        assert!(!acct.open_orders[0].is_buy); // order(false)
        assert_eq!(acct.perp_usdc, Some(Decimal::new("500"))); // merge, not replace
    }
}
