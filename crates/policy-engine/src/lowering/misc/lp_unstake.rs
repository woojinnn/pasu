use crate::action::misc::LpUnstakeAction;
use crate::context_keys::{GAUGE, LP_TOKEN, RECIPIENT};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::misc::asset_with_amount_json;
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "lp_unstake";

impl Lower for LpUnstakeAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(action: &LpUnstakeAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(GAUGE.into(), Value::from(action.gauge.to_string()));
    context.insert(LP_TOKEN.into(), asset_with_amount_json(&action.lp_token)?);
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    Ok(Value::Object(context))
}
