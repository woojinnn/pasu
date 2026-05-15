use crate::action::dex::{RemoveLiquidityAction, RemoveLiquidityExitMode};
use crate::context_keys::{EXIT_MODE, INPUT_LP, OUTPUT_TOKENS, POOL, RECIPIENT};
use crate::lowering::dex::{asset_with_amount_json, asset_with_amounts_json};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::pool::pool_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "remove_liquidity";
const VALIDITY: &str = "validity";

impl Lower for RemoveLiquidityAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        let mut context = Map::new();
        context.insert(
            EXIT_MODE.into(),
            Value::from(exit_mode_str(&self.exit_mode)),
        );
        context.insert(POOL.into(), pool_json(&self.pool));
        context.insert(INPUT_LP.into(), asset_with_amount_json(&self.input_lp)?);
        context.insert(
            OUTPUT_TOKENS.into(),
            asset_with_amounts_json(&self.outputs)?,
        );
        context.insert(RECIPIENT.into(), Value::from(self.recipient.to_string()));
        if let Some(validity) = &self.validity {
            context.insert(VALIDITY.into(), validity_json(validity));
        }

        Ok(ctx.request(ACTION_ID, Value::Object(context)))
    }
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
    use crate::action::{Action, AmountKind, AssetRefWithAmountConstraint};

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
                pool: pool(),
                input_lp: AssetRefWithAmountConstraint {
                    asset: erc20("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "UNI-V2", 18),
                    amount: amount(AmountKind::Exact, "1000"),
                },
                outputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
                recipient: from.clone(),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("remove_liquidity"));
        assert!(request.context.get("exitMode").is_some());
        assert!(request.context.get("pool").is_some());
        assert!(request.context.get("inputLp").is_some());
        assert!(request.context.get("outputTokens").is_some());
        assert!(request.context.get("recipient").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
