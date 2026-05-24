use crate::action::misc::LpUnstakeAction;
use crate::context_keys::{AMOUNT, GAUGE, LP_TOKEN, RECIPIENT};
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
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
    context.insert(LP_TOKEN.into(), asset_ref_json(&action.lp_token)?);
    context.insert(AMOUNT.into(), amount_constraint_json(&action.amount));
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    Ok(Value::Object(context))
}
