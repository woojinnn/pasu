use crate::action::misc::LockCreateAction;
use crate::context_keys::{AMOUNT, ASSET_FIELD, LOCK_DURATION_SEC, RECIPIENT, VOTING_ESCROW};
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
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
    context.insert(ASSET_FIELD.into(), asset_ref_json(&action.asset)?);
    context.insert(AMOUNT.into(), amount_constraint_json(&action.amount));
    context.insert(
        LOCK_DURATION_SEC.into(),
        Value::from(action.lock_duration_sec.to_string()),
    );
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    Ok(Value::Object(context))
}
