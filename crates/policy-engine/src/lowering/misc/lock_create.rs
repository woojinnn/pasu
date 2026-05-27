use crate::action::misc::LockCreateAction;
use crate::context_keys::{ASSET_FIELD, LOCK_DURATION_SEC, RECIPIENT, UNLOCK_TIME, VOTING_ESCROW};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::misc::asset_with_amount_json;
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "lock_create";

impl Lower for LockCreateAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(action: &LockCreateAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(
        VOTING_ESCROW.into(),
        Value::from(action.voting_escrow.to_string()),
    );
    context.insert(ASSET_FIELD.into(), asset_with_amount_json(&action.asset)?);
    if let Some(lock_duration_sec) = &action.lock_duration_sec {
        context.insert(
            LOCK_DURATION_SEC.into(),
            Value::from(lock_duration_sec.to_string()),
        );
    }
    if let Some(unlock_time) = &action.unlock_time {
        context.insert(UNLOCK_TIME.into(), Value::from(unlock_time.to_string()));
    }
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    Ok(Value::Object(context))
}
