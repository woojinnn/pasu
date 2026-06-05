//! `hl_update_leverage` reducer — upsert a per-asset leverage setting.

use policy_state::position::{HlAccount, HlLeverageSetting, PositionKind};
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlUpdateLeverageAction;
use crate::error::ReducerResult;
use crate::helpers;

use super::common;

pub(super) fn apply(
    action: &HlUpdateLeverageAction,
    state: &WalletState,
    ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    let setting = HlLeverageSetting {
        asset_index: action.asset_index,
        is_cross: action.is_cross,
        leverage: action.leverage,
    };

    let mut delta = StateDelta::new();

    if common::find_hl_account(state).is_some() {
        let id = common::HL_ACCOUNT_ID.to_owned();
        helpers::position::upsert_hl_account(state, &mut delta, &id, |pos| {
            if let PositionKind::HyperliquidAccount(a) = &mut pos.kind {
                upsert_setting(&mut a.leverage_settings, setting.clone());
            }
        })?;
    } else {
        let acct = HlAccount {
            leverage_settings: vec![setting],
            ..HlAccount::default()
        };
        helpers::position::open_position(state, &mut delta, common::hl_position(acct, ctx.now))?;
    }
    Ok(delta)
}

/// Replace an existing setting for the same `asset_index`, or append.
fn upsert_setting(settings: &mut Vec<HlLeverageSetting>, new: HlLeverageSetting) {
    if let Some(slot) = settings
        .iter_mut()
        .find(|s| s.asset_index == new.asset_index)
    {
        *slot = new;
    } else {
        settings.push(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use policy_state::eval_context::RequestKind;
    use policy_state::position::{HlAccount, HlLeverageSetting, Position, PositionKind};
    use policy_state::primitives::{Address, ChainId, Time};
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
    fn act(lev: u32) -> HlUpdateLeverageAction {
        HlUpdateLeverageAction {
            asset_index: 0,
            symbol: Some("BTC".to_owned()),
            is_cross: true,
            leverage: lev,
        }
    }
    fn state_with(settings: Vec<HlLeverageSetting>) -> WalletState {
        let mut s = empty_state();
        s.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: super::super::common::hl_protocol_ref(),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                leverage_settings: settings,
                ..HlAccount::default()
            }),
            primitives_synced_at: Time::from_unix(1),
            primitives_source: policy_state::live_field::DataSource::UserSupplied,
        });
        s
    }

    #[test]
    fn update_leverage_on_empty_base_opens_account() {
        let delta = apply(&act(5), &empty_state(), &ctx()).unwrap();
        match &delta.position_changes[0] {
            PositionChange::Open { position } => match &position.kind {
                PositionKind::HyperliquidAccount(a) => {
                    assert_eq!(a.leverage_settings.len(), 1);
                    assert_eq!(a.leverage_settings[0].leverage, 5);
                }
                o => panic!("{o:?}"),
            },
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn update_leverage_upserts_same_asset() {
        // Existing setting for asset 0 at 5x; re-set to 10x → still ONE setting, value 10.
        let base = vec![HlLeverageSetting {
            asset_index: 0,
            is_cross: true,
            leverage: 5,
        }];
        let delta = apply(&act(10), &state_with(base), &ctx()).unwrap();
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));
        // The Update patch carries the full snapshot; verify by applying it.
        let next = crate::helpers::delta::apply_delta(
            &state_with(vec![HlLeverageSetting {
                asset_index: 0,
                is_cross: true,
                leverage: 5,
            }]),
            &delta,
        )
        .unwrap();
        let acct = next
            .positions
            .iter()
            .find_map(|p| match &p.kind {
                PositionKind::HyperliquidAccount(a) => Some(a.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(acct.leverage_settings.len(), 1);
        assert_eq!(acct.leverage_settings[0].leverage, 10);
    }

    #[test]
    fn update_leverage_appends_new_asset() {
        // Base holds a setting for asset 0 @ 5x; apply a change for asset 1 @ 7x.
        let base = vec![HlLeverageSetting {
            asset_index: 0,
            is_cross: true,
            leverage: 5,
        }];
        let action = HlUpdateLeverageAction {
            asset_index: 1, // different asset than base → forces the append branch
            symbol: Some("ETH".to_owned()),
            is_cross: false,
            leverage: 7,
        };
        let delta = apply(&action, &state_with(base.clone()), &ctx()).unwrap();
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));

        let next = crate::helpers::delta::apply_delta(&state_with(base), &delta).unwrap();
        let acct = next
            .positions
            .iter()
            .find_map(|p| match &p.kind {
                PositionKind::HyperliquidAccount(a) => Some(a.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(acct.leverage_settings.len(), 2); // appended, not replaced
        let s0 = acct
            .leverage_settings
            .iter()
            .find(|s| s.asset_index == 0)
            .unwrap();
        assert_eq!(s0.leverage, 5); // original untouched
        let s1 = acct
            .leverage_settings
            .iter()
            .find(|s| s.asset_index == 1)
            .unwrap();
        assert_eq!(s1.leverage, 7);
        assert!(!s1.is_cross);
    }
}
