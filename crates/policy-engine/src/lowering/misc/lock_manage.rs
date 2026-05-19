use crate::action::misc::{LockManageAction, LockManageKind};
use crate::context_keys::{FROM_TOKEN_ID, KIND, SPLIT_RATIO, TO_TOKEN_ID, VOTING_ESCROW};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "lock_manage";

impl Lower for LockManageAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

const fn lock_manage_kind_str(kind: &LockManageKind) -> &'static str {
    match kind {
        LockManageKind::Merge => "merge",
        LockManageKind::Split => "split",
    }
}

fn context(action: &LockManageAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(
        VOTING_ESCROW.into(),
        Value::from(action.voting_escrow.to_string()),
    );
    context.insert(KIND.into(), Value::from(lock_manage_kind_str(&action.kind)));
    context.insert(
        FROM_TOKEN_ID.into(),
        Value::from(action.from_token_id.to_string()),
    );
    if let Some(to_token_id) = &action.to_token_id {
        context.insert(TO_TOKEN_ID.into(), Value::from(to_token_id.to_string()));
    }
    if let Some(split_ratio) = &action.split_ratio {
        context.insert(SPLIT_RATIO.into(), Value::from(split_ratio.to_string()));
    }
    Ok(Value::Object(context))
}
