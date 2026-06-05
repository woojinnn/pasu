//! `Governance::ActivateVoting` lowering.

use serde_json::{Map, Value};

use policy_transition::action::governance::GovernanceProposalRefAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_governance_venue, lower_proposal_id};

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GovernanceProposalRefAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_governance_venue(&action.venue));
    m.insert("proposalId".into(), lower_proposal_id(action.proposal_id));

    Ok(ctx.lowered(r#"Governance::Action::"ActivateVoting""#, Value::Object(m)))
}
