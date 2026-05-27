use crate::action::misc::LockWithdrawAction;
use crate::context_keys::{ASSET_FIELD, RECIPIENT, TOKEN_ID, VOTING_ESCROW};
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "lock_withdraw";

impl Lower for LockWithdrawAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(action: &LockWithdrawAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(
        VOTING_ESCROW.into(),
        Value::from(action.voting_escrow.to_string()),
    );
    if let Some(token_id) = &action.token_id {
        context.insert(TOKEN_ID.into(), Value::from(token_id.to_string()));
    }
    context.insert(ASSET_FIELD.into(), asset_ref_json(&action.asset)?);
    context.insert(
        RECIPIENT.into(),
        Value::from(action.recipient.to_string()),
    );
    Ok(Value::Object(context))
}
