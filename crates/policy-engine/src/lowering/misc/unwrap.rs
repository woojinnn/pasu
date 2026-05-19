use crate::action::misc::UnwrapAction;
use crate::context_keys::RECIPIENT;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::misc::asset_with_amount_json;
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "unwrap";
const WRAPPED_ASSET: &str = "wrappedAsset";
const NATIVE_ASSET: &str = "nativeAsset";

impl Lower for UnwrapAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(u: &UnwrapAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(WRAPPED_ASSET.into(), asset_with_amount_json(&u.wrapped_asset)?);
    context.insert(NATIVE_ASSET.into(), asset_with_amount_json(&u.native_asset)?);
    context.insert(RECIPIENT.into(), Value::from(u.recipient.to_string()));
    Ok(Value::Object(context))
}
