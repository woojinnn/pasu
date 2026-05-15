use crate::action::dex::DonateAction;
use crate::context_keys::{
    FROM, HOOKS, HOOK_DATA_LEN, HOOK_DATA_SELECTOR, HOOK_PERMISSIONS, INPUT_TOKENS, IS_DYNAMIC_FEE,
    POOL,
};
use crate::lowering::common::cedar::cedar_long_u64;
use crate::lowering::common::pool::pool_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dex::asset_with_amounts_json;
use crate::lowering::dex::hooks::hook_permissions_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "donate";
const VALIDITY: &str = "validity";

impl Lower for DonateAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        let mut context = Map::new();
        context.insert(POOL.into(), pool_json(&self.pool));
        context.insert(
            INPUT_TOKENS.into(),
            asset_with_amounts_json(&self.input_tokens)?,
        );
        if let Some(from) = &self.from {
            context.insert(FROM.into(), Value::from(from.to_string()));
        }
        if let Some(validity) = &self.validity {
            context.insert(VALIDITY.into(), validity_json(validity));
        }
        if let Some(hooks) = &self.hooks {
            context.insert(HOOKS.into(), Value::from(hooks.to_string()));
        }
        if let Some(permissions) = &self.hook_permissions {
            context.insert(HOOK_PERMISSIONS.into(), hook_permissions_json(permissions));
        }
        if let Some(is_dynamic_fee) = self.is_dynamic_fee {
            context.insert(IS_DYNAMIC_FEE.into(), Value::Bool(is_dynamic_fee));
        }
        if let Some(hook_data_len) = self.hook_data_len {
            context.insert(HOOK_DATA_LEN.into(), cedar_long_u64(hook_data_len));
        }
        if let Some(selector) = &self.hook_data_selector {
            context.insert(HOOK_DATA_SELECTOR.into(), Value::from(selector.to_string()));
        }

        Ok(ctx.request(ACTION_ID, Value::Object(context)))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::dex::DonateAction;
    use crate::action::{Action, AmountKind};

    use crate::lowering::dex::test_support::{
        address, asset_amount_pair, envelope, policy_request, pool,
    };

    #[test]
    fn donate_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::Donate(DonateAction {
                pool: pool(),
                input_tokens: asset_amount_pair(AmountKind::Exact, AmountKind::Exact),
                from: None,
                validity: None,
                hooks: None,
                hook_permissions: None,
                is_dynamic_fee: None,
                hook_data_len: None,
                hook_data_selector: None,
            })),
            &from,
        );

        assert!(request.action.contains("donate"));
        assert!(request.context.get("pool").is_some());
        assert!(request.context.get("inputTokens").is_some());
    }
}
