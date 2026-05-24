use crate::action::staking::StakeAction;
use crate::context_keys::RECIPIENT;
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::{asset_ref_json, LoweringError};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "stake";
const TOKEN_IN: &str = "tokenIn";
const RECEIPT_TOKEN: &str = "receiptToken";
const AMOUNT_IN: &str = "amountIn";
const AMOUNT_OUT: &str = "amountOut";

impl Lower for StakeAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(action: &StakeAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(TOKEN_IN.into(), asset_ref_json(&action.token_in)?);
    context.insert(RECEIPT_TOKEN.into(), asset_ref_json(&action.receipt_token)?);
    context.insert(AMOUNT_IN.into(), amount_constraint_json(&action.amount_in));
    if let Some(amount_out) = &action.amount_out {
        context.insert(AMOUNT_OUT.into(), amount_constraint_json(amount_out));
    }
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    Ok(Value::Object(context))
}
