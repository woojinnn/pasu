use crate::action::misc::TransferAction;
use crate::context_keys::{FROM, RECIPIENT};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::misc::asset_with_amount_json;
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "transfer";
const TOKEN: &str = "token";

impl Lower for TransferAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(t: &TransferAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(TOKEN.into(), asset_with_amount_json(&t.token)?);
    context.insert(FROM.into(), Value::from(t.from.to_string()));
    context.insert(RECIPIENT.into(), Value::from(t.recipient.to_string()));
    Ok(Value::Object(context))
}
