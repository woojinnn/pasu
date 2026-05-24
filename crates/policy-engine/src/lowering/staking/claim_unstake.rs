use crate::action::staking::ClaimUnstakeAction;
use crate::context_keys::RECIPIENT;
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::{asset_ref_json, LoweringError};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "claim_unstake";
const TOKEN_OUT: &str = "tokenOut";
const AMOUNT_OUT: &str = "amountOut";

impl Lower for ClaimUnstakeAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(action: &ClaimUnstakeAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(TOKEN_OUT.into(), asset_ref_json(&action.token_out)?);
    if let Some(amount_out) = &action.amount_out {
        context.insert(AMOUNT_OUT.into(), amount_constraint_json(amount_out));
    }
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    Ok(Value::Object(context))
}
