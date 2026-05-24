use crate::action::misc::{VoteAction, VoteSupport};
use crate::context_keys::VALIDITY_DELTA_SEC;
use crate::lowering::common::asset::LoweringError;
use crate::lowering::common::validity::{validity_delta_sec, validity_json};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "vote";
const GOVERNANCE: &str = "governance";
const GOVERNANCE_LABEL: &str = "governanceLabel";
const PROPOSAL_ID: &str = "proposalId";
const SUPPORT: &str = "support";
const REASON: &str = "reason";
const VOTING_POWER: &str = "votingPower";
const VALIDITY: &str = "validity";

impl Lower for VoteAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self, ctx)))
    }
}

fn context(action: &VoteAction, ctx: &LoweringCtx<'_>) -> Value {
    let mut context = Map::new();
    context.insert(
        GOVERNANCE.into(),
        Value::from(action.governance.to_string()),
    );
    if let Some(label) = &action.governance_label {
        context.insert(GOVERNANCE_LABEL.into(), Value::from(label.as_str()));
    }
    context.insert(
        PROPOSAL_ID.into(),
        Value::from(action.proposal_id.to_string()),
    );
    context.insert(
        SUPPORT.into(),
        Value::from(vote_support_str(&action.support)),
    );
    if let Some(reason) = &action.reason {
        context.insert(REASON.into(), Value::from(reason.as_str()));
    }
    if let Some(voting_power) = &action.voting_power {
        context.insert(VOTING_POWER.into(), Value::from(voting_power.to_string()));
    }
    if let Some(validity) = &action.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
        if let Some(delta_sec) = validity_delta_sec(validity, ctx.block_timestamp) {
            context.insert(VALIDITY_DELTA_SEC.into(), Value::from(delta_sec));
        }
    }
    Value::Object(context)
}

const fn vote_support_str(support: &VoteSupport) -> &'static str {
    match support {
        VoteSupport::For => "for",
        VoteSupport::Against => "against",
        VoteSupport::Abstain => "abstain",
    }
}
