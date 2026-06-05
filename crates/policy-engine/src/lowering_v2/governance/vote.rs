//! `Governance::Vote` lowering.

use serde_json::{Map, Value};

use policy_transition::action::governance::GovernanceVoteAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_governance_venue, lower_proposal_id};

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GovernanceVoteAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_governance_venue(&action.venue));
    m.insert("proposalId".into(), lower_proposal_id(action.proposal_id));
    m.insert("support".into(), Value::Bool(action.support));
    if let Some(reason) = &action.reason {
        m.insert("reason".into(), Value::String(reason.clone()));
    }

    Ok(ctx.lowered(r#"Governance::Action::"Vote""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::governance::{GovernanceAction, GovernanceVoteAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{aave_voting_machine, assert_conforms, onchain_meta};

    #[test]
    fn vote_lowering_conforms() {
        let body = ActionBody::Governance(GovernanceAction::Vote(GovernanceVoteAction {
            venue: aave_voting_machine(),
            proposal_id: U256::from(42u64),
            support: true,
            reason: Some("support".into()),
        }));
        assert_conforms("vote", &body, &onchain_meta());
    }
}
