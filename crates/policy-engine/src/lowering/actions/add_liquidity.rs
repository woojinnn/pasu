use crate::action::dex::AddLiquidityAction;
use crate::context_keys::{INPUTS, LP_AMOUNT, LP_TOKEN, POOL, RECIPIENT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use super::request;
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::{asset_ref_json, asset_ref_with_amount_json};
use crate::lowering::common::pool::pool_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::LoweringCtx;

const ACTION_ID: &str = "add_liquidity";
const VALIDITY: &str = "validity";

pub(crate) fn build(action: &AddLiquidityAction, ctx: &LoweringCtx<'_>) -> PolicyRequest {
    let mut context = Map::new();
    if let Some(pool) = &action.pool {
        context.insert(POOL.into(), pool_json(pool));
    }
    context.insert(
        INPUTS.into(),
        Value::Array(
            action
                .inputs
                .iter()
                .map(asset_ref_with_amount_json)
                .collect(),
        ),
    );
    if let Some(lp_token) = &action.lp_token {
        context.insert(LP_TOKEN.into(), asset_ref_json(lp_token));
    }
    if let Some(lp_amount) = &action.lp_amount {
        context.insert(LP_AMOUNT.into(), amount_constraint_json(lp_amount));
    }
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    if let Some(validity) = &action.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
    }

    request(ACTION_ID, ctx, Value::Object(context))
}

#[cfg(test)]
mod tests {
    use crate::action::dex::AddLiquidityAction;
    use crate::action::{Action, AmountKind};

    use crate::lowering::actions::test_support::{
        address, amount, asset_amount_pair, envelope, erc20, policy_request, pool, validity,
        BLOCK_TIMESTAMP,
    };

    #[test]
    fn add_liquidity_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::AddLiquidity(AddLiquidityAction {
                pool: Some(pool()),
                inputs: asset_amount_pair(AmountKind::Max, AmountKind::Max),
                lp_token: Some(erc20(
                    "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "UNI-V2",
                    18,
                )),
                lp_amount: Some(amount(AmountKind::Min, "1000")),
                recipient: from.clone(),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("add_liquidity"));
        assert!(request.context.get("pool").is_some());
        assert!(request.context.get("inputs").is_some());
        assert!(request.context.get("lpToken").is_some());
        assert!(request.context.get("lpAmount").is_some());
        assert!(request.context.get("recipient").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
