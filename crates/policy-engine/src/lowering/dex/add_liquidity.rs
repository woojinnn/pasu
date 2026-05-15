use crate::action::dex::AddLiquidityAction;
use crate::context_keys::{INPUT_TOKENS, OUTPUT_LP, POOL, RECIPIENT};
use crate::lowering::dex::{asset_with_amount_json, asset_with_amounts_json};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::pool::pool_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "add_liquidity";
const VALIDITY: &str = "validity";

impl Lower for AddLiquidityAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        let mut context = Map::new();
        context.insert(POOL.into(), pool_json(&self.pool));
        context.insert(INPUT_TOKENS.into(), asset_with_amounts_json(&self.inputs)?);
        context.insert(OUTPUT_LP.into(), asset_with_amount_json(&self.output_lp)?);
        context.insert(RECIPIENT.into(), Value::from(self.recipient.to_string()));
        if let Some(validity) = &self.validity {
            context.insert(VALIDITY.into(), validity_json(validity));
        }

        Ok(ctx.request(ACTION_ID, Value::Object(context)))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::dex::AddLiquidityAction;
    use crate::action::{Action, AmountKind, AssetRefWithAmountConstraint};

    use crate::lowering::dex::test_support::{
        address, amount, asset_amount_pair, envelope, erc20, policy_request, pool, validity,
        BLOCK_TIMESTAMP,
    };

    #[test]
    fn add_liquidity_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::AddLiquidity(AddLiquidityAction {
                pool: pool(),
                inputs: asset_amount_pair(AmountKind::Max, AmountKind::Max),
                output_lp: AssetRefWithAmountConstraint {
                    asset: erc20("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "UNI-V2", 18),
                    amount: amount(AmountKind::Min, "1000"),
                },
                recipient: from.clone(),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("add_liquidity"));
        assert!(request.context.get("pool").is_some());
        assert!(request.context.get("inputTokens").is_some());
        assert!(request.context.get("outputLp").is_some());
        assert!(request.context.get("recipient").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
