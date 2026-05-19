use crate::action::misc::{GaugeVoteAction, GaugeVoteKind};
use crate::context_keys::{KIND, POOLS, TOKEN_ID, VOTER, WEIGHTS, WEIGHTS_SUM};
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use alloy_primitives::U256;
use serde_json::{Map, Value};

const ACTION_ID: &str = "gauge_vote";
const VALIDITY: &str = "validity";

impl Lower for GaugeVoteAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

const fn gauge_vote_kind_str(kind: &GaugeVoteKind) -> &'static str {
    match kind {
        GaugeVoteKind::Vote => "vote",
        GaugeVoteKind::Reset => "reset",
        GaugeVoteKind::Poke => "poke",
    }
}

fn context(action: &GaugeVoteAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(VOTER.into(), Value::from(action.voter.to_string()));
    context.insert(TOKEN_ID.into(), Value::from(action.token_id.to_string()));

    let pools = action
        .pools
        .iter()
        .map(|addr| Value::from(addr.to_string()))
        .collect::<Vec<_>>();
    context.insert(POOLS.into(), Value::from(pools));

    let weights = action
        .weights
        .iter()
        .map(|w| Value::from(w.to_string()))
        .collect::<Vec<_>>();
    context.insert(WEIGHTS.into(), Value::from(weights));

    // Derived: weightsSum (string-encoded U256). Saturating add keeps the
    // policy contract honest even if upstream emits a degenerate sum.
    let mut sum = U256::ZERO;
    for w in &action.weights {
        if let Ok(parsed) = U256::from_str_radix(&w.to_string(), 10) {
            sum = sum.saturating_add(parsed);
        }
    }
    context.insert(WEIGHTS_SUM.into(), Value::from(sum.to_string()));

    let kind = action
        .kind
        .as_ref()
        .map_or("vote", gauge_vote_kind_str);
    context.insert(KIND.into(), Value::from(kind));

    if let Some(validity) = &action.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
    }

    Ok(Value::Object(context))
}
