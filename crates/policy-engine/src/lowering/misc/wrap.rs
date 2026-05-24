use crate::action::misc::WrapAction;
use crate::context_keys::RECIPIENT;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::misc::asset_with_amount_json;
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "wrap";
const NATIVE_ASSET: &str = "nativeAsset";
const WRAPPED_ASSET: &str = "wrappedAsset";

impl Lower for WrapAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(w: &WrapAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(
        NATIVE_ASSET.into(),
        asset_with_amount_json(&w.native_asset)?,
    );
    context.insert(
        WRAPPED_ASSET.into(),
        asset_with_amount_json(&w.wrapped_asset)?,
    );
    context.insert(RECIPIENT.into(), Value::from(w.recipient.to_string()));
    Ok(Value::Object(context))
}
