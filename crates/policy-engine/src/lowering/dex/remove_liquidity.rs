use crate::action::dex::{RemoveLiquidityAction, RemoveLiquidityExitMode};
use crate::context_keys::{EXIT_MODE, LP_BURN_AMOUNT, LP_TOKEN, OUTPUTS, POOL, RECIPIENT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use super::request;
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::{asset_ref_json, asset_ref_with_amount_json};
use crate::lowering::common::pool::pool_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::LoweringCtx;

const ACTION_ID: &str = "remove_liquidity";
const VALIDITY: &str = "validity";

pub(crate) fn build(action: &RemoveLiquidityAction, ctx: &LoweringCtx<'_>) -> PolicyRequest {
    let mut context = Map::new();
    context.insert(
        EXIT_MODE.into(),
        Value::from(exit_mode_str(&action.exit_mode)),
    );
    if let Some(pool) = &action.pool {
        context.insert(POOL.into(), pool_json(pool));
    }
    if let Some(lp_token) = &action.lp_token {
        context.insert(LP_TOKEN.into(), asset_ref_json(lp_token));
    }
    if let Some(lp_burn_amount) = &action.lp_burn_amount {
        context.insert(
            LP_BURN_AMOUNT.into(),
            amount_constraint_json(lp_burn_amount),
        );
    }
    context.insert(
        OUTPUTS.into(),
        Value::Array(
            action
                .outputs
                .iter()
                .map(asset_ref_with_amount_json)
                .collect(),
        ),
    );
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));
    if let Some(validity) = &action.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
    }

    request(ACTION_ID, ctx, Value::Object(context))
}

const fn exit_mode_str(mode: &RemoveLiquidityExitMode) -> &'static str {
    match mode {
        RemoveLiquidityExitMode::Proportional => "proportional",
        RemoveLiquidityExitMode::SingleAsset => "single_asset",
        RemoveLiquidityExitMode::ExactOut => "exact_out",
    }
}

#[cfg(test)]
mod tests {
    use crate::action::dex::{RemoveLiquidityAction, RemoveLiquidityExitMode};
    use crate::action::{Action, AmountKind};

    use crate::lowering::dex::test_support::{
        address, amount, asset_amount_pair, envelope, erc20, policy_request, pool, validity,
        BLOCK_TIMESTAMP,
    };

    #[test]
    fn remove_liquidity_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::RemoveLiquidity(RemoveLiquidityAction {
                exit_mode: RemoveLiquidityExitMode::Proportional,
                pool: Some(pool()),
                lp_token: Some(erc20(
                    "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "UNI-V2",
                    18,
                )),
                lp_burn_amount: Some(amount(AmountKind::Exact, "1000")),
                outputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
                recipient: from.clone(),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("remove_liquidity"));
        assert!(request.context.get("exitMode").is_some());
        assert!(request.context.get("pool").is_some());
        assert!(request.context.get("lpToken").is_some());
        assert!(request.context.get("lpBurnAmount").is_some());
        assert!(request.context.get("outputs").is_some());
        assert!(request.context.get("recipient").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
