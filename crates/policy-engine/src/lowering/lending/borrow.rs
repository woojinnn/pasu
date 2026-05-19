use crate::action::lending::{AmountMode, BorrowAction};
use crate::context_keys::{AMOUNT, ASSET, RECIPIENT, VALIDITY_DELTA_SEC};
use crate::lowering::common::asset::{asset_ref_json, LoweringError};
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::validity::{validity_delta_sec, validity_json};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::market_json;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "borrow";
const MARKET: &str = "market";
const AMOUNT_MODE: &str = "amountMode";
const ON_BEHALF: &str = "onBehalf";
const VALIDITY: &str = "validity";

impl Lower for BorrowAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self, ctx)?))
    }
}

fn context(action: &BorrowAction, ctx: &LoweringCtx<'_>) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    if let Some(market) = &action.market {
        context.insert(MARKET.into(), market_json(market));
    }
    context.insert(ASSET.into(), asset_ref_json(&action.asset)?);
    context.insert(AMOUNT.into(), amount_constraint_json(&action.amount));
    if let Some(mode) = &action.amount_mode {
        context.insert(AMOUNT_MODE.into(), Value::from(amount_mode_str(mode)));
    }
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    context.insert(ON_BEHALF.into(), Value::from(action.on_behalf.to_string()));
    if let Some(validity) = &action.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
        if let Some(delta_sec) = validity_delta_sec(validity, ctx.block_timestamp) {
            context.insert(VALIDITY_DELTA_SEC.into(), Value::from(delta_sec));
        }
    }
    Ok(Value::Object(context))
}

const fn amount_mode_str(mode: &AmountMode) -> &'static str {
    match mode {
        AmountMode::Assets => "assets",
        AmountMode::Shares => "shares",
    }
}
