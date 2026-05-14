use crate::action::dex::IncreaseLiquidityAction;
use crate::context_keys::{INPUTS, NFT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use super::request;
use crate::lowering::common::asset::{asset_ref_json, asset_ref_with_amount_json};
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "increase_liquidity";
const VALIDITY: &str = "validity";

impl Lower for IncreaseLiquidityAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        context.insert(NFT.into(), asset_ref_json(&self.nft));
        context.insert(
            INPUTS.into(),
            Value::Array(
                self.inputs
                    .iter()
                    .map(asset_ref_with_amount_json)
                    .collect(),
            ),
        );
        if let Some(validity) = &self.validity {
            context.insert(VALIDITY.into(), validity_json(validity));
        }

        request(ACTION_ID, ctx, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::dex::IncreaseLiquidityAction;
    use crate::action::{Action, AmountKind};

    use crate::lowering::dex::test_support::{
        address, asset_amount_pair, envelope, nft, policy_request, validity, BLOCK_TIMESTAMP,
    };

    #[test]
    fn increase_liquidity_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::IncreaseLiquidity(IncreaseLiquidityAction {
                nft: nft("42"),
                inputs: asset_amount_pair(AmountKind::Max, AmountKind::Max),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("increase_liquidity"));
        assert!(request.context.get("nft").is_some());
        assert!(request.context.get("inputs").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
