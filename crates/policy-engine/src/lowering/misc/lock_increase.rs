use crate::action::misc::{LockIncreaseAction, LockIncreaseKind};
use crate::context_keys::{
    ADDITIONAL_AMOUNT, KIND, NEW_LOCK_DURATION_SEC, NEW_UNLOCK_TIME, RECIPIENT, TOKEN_ID,
    VOTING_ESCROW,
};
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "lock_increase";

impl Lower for LockIncreaseAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)))
    }
}

const fn lock_increase_kind_str(kind: LockIncreaseKind) -> &'static str {
    match kind {
        LockIncreaseKind::Amount => "amount",
        LockIncreaseKind::UnlockTime => "unlock_time",
    }
}

fn context(action: &LockIncreaseAction) -> Value {
    let mut context = Map::new();
    context.insert(
        VOTING_ESCROW.into(),
        Value::from(action.voting_escrow.to_string()),
    );
    if let Some(token_id) = &action.token_id {
        context.insert(TOKEN_ID.into(), Value::from(token_id.to_string()));
    }
    context.insert(
        KIND.into(),
        Value::from(lock_increase_kind_str(action.kind)),
    );
    if let Some(additional_amount) = &action.additional_amount {
        context.insert(
            ADDITIONAL_AMOUNT.into(),
            amount_constraint_json(additional_amount),
        );
    }
    if let Some(new_lock_duration_sec) = &action.new_lock_duration_sec {
        context.insert(
            NEW_LOCK_DURATION_SEC.into(),
            Value::from(new_lock_duration_sec.to_string()),
        );
    }
    if let Some(new_unlock_time) = &action.new_unlock_time {
        context.insert(
            NEW_UNLOCK_TIME.into(),
            Value::from(new_unlock_time.to_string()),
        );
    }
    if let Some(recipient) = &action.recipient {
        context.insert(RECIPIENT.into(), Value::from(recipient.to_string()));
    }
    Value::Object(context)
}
