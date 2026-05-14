use crate::action::dex::DecreaseLiquidityAction;
use crate::context_keys::{LIQUIDITY_DELTA, NFT, OUTPUTS, RECIPIENT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use super::request;
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::{asset_ref_json, asset_ref_with_amount_json};
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "decrease_liquidity";
const VALIDITY: &str = "validity";

impl Lower for DecreaseLiquidityAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        context.insert(NFT.into(), asset_ref_json(&self.nft));
        context.insert(
            LIQUIDITY_DELTA.into(),
            amount_constraint_json(&self.liquidity_delta),
        );
        context.insert(
            OUTPUTS.into(),
            Value::Array(
                self.outputs
                    .iter()
                    .map(asset_ref_with_amount_json)
                    .collect(),
            ),
        );
        if let Some(recipient) = &self.recipient {
            context.insert(RECIPIENT.into(), Value::from(recipient.to_string()));
        }
        if let Some(validity) = &self.validity {
            context.insert(VALIDITY.into(), validity_json(validity));
        }

        request(ACTION_ID, ctx, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::dex::DecreaseLiquidityAction;
    use crate::action::{Action, AmountKind};

    use crate::lowering::dex::test_support::{
        address, amount, asset_amount_pair, envelope, nft, policy_request, validity,
        BLOCK_TIMESTAMP,
    };

    #[test]
    fn decrease_liquidity_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::DecreaseLiquidity(DecreaseLiquidityAction {
                nft: nft("42"),
                liquidity_delta: amount(AmountKind::Exact, "1000"),
                outputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
                recipient: Some(from.clone()),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("decrease_liquidity"));
        assert!(request.context.get("nft").is_some());
        assert!(request.context.get("liquidityDelta").is_some());
        assert!(request.context.get("outputs").is_some());
        assert!(request.context.get("recipient").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
