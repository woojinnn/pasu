use crate::action::dex::{MintLiquidityNftAction, TickRange};
use crate::context_keys::{FEE_TIER_BPS, INPUTS, LOWER, NFT, POOL, RECIPIENT, TICK_RANGE, UPPER};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::asset::{asset_ref_json, asset_ref_with_amount_json};
use crate::lowering::common::cedar::cedar_long_u64;
use crate::lowering::common::pool::pool_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "mint_liquidity_nft";
const VALIDITY: &str = "validity";

impl Lower for MintLiquidityNftAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(pool) = &self.pool {
            context.insert(POOL.into(), pool_json(pool));
        }
        context.insert(
            FEE_TIER_BPS.into(),
            cedar_long_u64(u64::from(self.fee_tier_bps)),
        );
        context.insert(TICK_RANGE.into(), tick_range_json(&self.tick_range));
        context.insert(
            INPUTS.into(),
            Value::Array(self.inputs.iter().map(asset_ref_with_amount_json).collect()),
        );
        context.insert(NFT.into(), asset_ref_json(&self.nft));
        context.insert(RECIPIENT.into(), Value::from(self.recipient.to_string()));
        if let Some(validity) = &self.validity {
            context.insert(VALIDITY.into(), validity_json(validity));
        }

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

fn tick_range_json(tick_range: &TickRange) -> Value {
    let mut out = Map::new();
    out.insert(LOWER.into(), Value::from(i64::from(tick_range.lower)));
    out.insert(UPPER.into(), Value::from(i64::from(tick_range.upper)));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use crate::action::dex::MintLiquidityNftAction;
    use crate::action::{Action, AmountKind};

    use crate::lowering::dex::test_support::{
        address, asset_amount_pair, envelope, nft, policy_request, pool, tick_range, validity,
        BLOCK_TIMESTAMP,
    };

    #[test]
    fn mint_liquidity_nft_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::MintLiquidityNft(MintLiquidityNftAction {
                pool: Some(pool()),
                fee_tier_bps: 5,
                tick_range: tick_range(),
                inputs: asset_amount_pair(AmountKind::Max, AmountKind::Max),
                nft: nft("42"),
                recipient: from.clone(),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("mint_liquidity_nft"));
        assert!(request.context.get("pool").is_some());
        assert!(request.context.get("feeTierBps").is_some());
        assert!(request.context.get("tickRange").is_some());
        assert!(request.context.get("inputs").is_some());
        assert!(request.context.get("nft").is_some());
        assert!(request.context.get("recipient").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
