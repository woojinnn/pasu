//! `hl_approve_agent` reducer — record a delegated agent (API) wallet.

use policy_state::position::{HlAccount, HlAgentApproval, PositionKind};
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlApproveAgentAction;
use crate::error::ReducerResult;
use crate::helpers;

use super::common;

pub(super) fn apply(
    action: &HlApproveAgentAction,
    state: &WalletState,
    ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    let agent = HlAgentApproval {
        agent_address: action.agent_address,
        agent_name: action.agent_name.clone(),
    };

    let mut delta = StateDelta::new();

    if common::find_hl_account(state).is_some() {
        let id = common::HL_ACCOUNT_ID.to_owned();
        helpers::position::upsert_hl_account(state, &mut delta, &id, |pos| {
            if let PositionKind::HyperliquidAccount(a) = &mut pos.kind {
                a.agents.push(agent.clone());
            }
        })?;
    } else {
        let acct = HlAccount {
            agents: vec![agent],
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
    fn act() -> HlApproveAgentAction {
        HlApproveAgentAction {
            agent_address: Address::from([0x33; 20]),
            agent_name: Some("bot".to_owned()),
        }
    }
    fn state_with_agents(n: usize) -> WalletState {
        let mut s = empty_state();
        let agents = (0..n)
            .map(|i| policy_state::position::HlAgentApproval {
                agent_address: Address::from([u8::try_from(i).unwrap(); 20]),
                agent_name: None,
            })
            .collect();
        s.positions.push(Position {
            id: HL_ACCOUNT_ID.to_owned(),
            protocol: super::super::common::hl_protocol_ref(),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                agents,
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
    fn approve_agent_on_empty_base_opens_account_with_agent() {
        let delta = apply(&act(), &empty_state(), &ctx()).unwrap();
        match &delta.position_changes[0] {
            PositionChange::Open { position } => match &position.kind {
                PositionKind::HyperliquidAccount(a) => {
                    assert_eq!(a.agents.len(), 1);
                    assert_eq!(a.agents[0].agent_name.as_deref(), Some("bot"));
                }
                o => panic!("{o:?}"),
            },
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn approve_agent_appends_to_existing() {
        let delta = apply(&act(), &state_with_agents(1), &ctx()).unwrap();
        assert!(matches!(
            delta.position_changes[0],
            PositionChange::Update { .. }
        ));
        let next = crate::helpers::delta::apply_delta(&state_with_agents(1), &delta).unwrap();
        let a = hl_of(&next);
        assert_eq!(a.agents.len(), 2); // appended, not replaced
                                       // the newly-approved agent is present...
        assert!(a
            .agents
            .iter()
            .any(|g| g.agent_address == Address::from([0x33; 20])
                && g.agent_name.as_deref() == Some("bot")));
        // ...and the pre-existing agent survived the merge.
        assert!(a
            .agents
            .iter()
            .any(|g| g.agent_address == Address::from([0x00; 20]) && g.agent_name.is_none()));
    }
}
