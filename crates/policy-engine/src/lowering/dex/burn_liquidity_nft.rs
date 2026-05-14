use crate::action::dex::{BurnKind, BurnLiquidityNftAction};
use crate::context_keys::{BURN_KIND, NFT, OUTPUTS, RECIPIENT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use super::request;
use crate::lowering::common::asset::{asset_ref_json, asset_ref_with_amount_json};
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::LoweringCtx;

const ACTION_ID: &str = "burn_liquidity_nft";
const VALIDITY: &str = "validity";

pub(crate) fn build(action: &BurnLiquidityNftAction, ctx: &LoweringCtx<'_>) -> PolicyRequest {
    let mut context = Map::new();
    context.insert(NFT.into(), asset_ref_json(&action.nft));
    context.insert(
        BURN_KIND.into(),
        Value::from(burn_kind_str(&action.burn_kind)),
    );
    if let Some(outputs) = &action.outputs {
        context.insert(
            OUTPUTS.into(),
            Value::Array(outputs.iter().map(asset_ref_with_amount_json).collect()),
        );
    }
    if let Some(recipient) = &action.recipient {
        context.insert(RECIPIENT.into(), Value::from(recipient.to_string()));
    }
    if let Some(validity) = &action.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
    }

    request(ACTION_ID, ctx, Value::Object(context))
}

const fn burn_kind_str(kind: &BurnKind) -> &'static str {
    match kind {
        BurnKind::EmptyOnly => "empty_only",
        BurnKind::AutoDecrease => "auto_decrease",
    }
}

#[cfg(test)]
mod tests {
    use crate::action::dex::{BurnKind, BurnLiquidityNftAction};
    use crate::action::{Action, AmountKind};

    use crate::lowering::dex::test_support::{
        address, asset_amount_pair, envelope, nft, policy_request, validity, BLOCK_TIMESTAMP,
    };

    #[test]
    fn burn_liquidity_nft_lowers_required_context_fields() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::BurnLiquidityNft(BurnLiquidityNftAction {
                nft: nft("42"),
                burn_kind: BurnKind::AutoDecrease,
                outputs: Some(asset_amount_pair(AmountKind::Min, AmountKind::Min)),
                recipient: Some(from.clone()),
                validity: Some(validity(BLOCK_TIMESTAMP + 600)),
            })),
            &from,
        );

        assert!(request.action.contains("burn_liquidity_nft"));
        assert!(request.context.get("nft").is_some());
        assert!(request.context.get("burnKind").is_some());
        assert!(request.context.get("outputs").is_some());
        assert!(request.context.get("recipient").is_some());
        assert!(request.context.get("validity").is_some());
    }
}
