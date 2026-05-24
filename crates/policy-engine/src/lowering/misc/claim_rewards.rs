use crate::action::misc::ClaimRewardsAction;
use crate::context_keys::{AMOUNT, ASSET, FROM, NFT, RECIPIENT};
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::{asset_ref_json, LoweringError};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "claim_rewards";
const SOURCE_ADDRESS: &str = "sourceAddress";
const SOURCE_LABEL: &str = "sourceLabel";
const REWARDS: &str = "rewards";

impl Lower for ClaimRewardsAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(action: &ClaimRewardsAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    // sourceAddress is required in the cedarschema. Fall back to the empty
    // string when the SourceRef carries no explicit address — keeps the
    // schema satisfied while preserving the "unknown source" signal.
    let (source_addr, source_label) = action.source.as_ref().map_or_else(
        || (String::new(), None),
        |source| {
            (
                source
                    .address
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                source.label.clone(),
            )
        },
    );
    context.insert(SOURCE_ADDRESS.into(), Value::from(source_addr));
    if let Some(label) = source_label {
        context.insert(SOURCE_LABEL.into(), Value::from(label));
    }
    if let Some(nft) = &action.nft {
        context.insert(NFT.into(), asset_ref_json(nft)?);
    }
    context.insert(FROM.into(), Value::from(action.from.to_string()));
    context.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));

    // The cedarschema declares rewards as `Set<AssetRefWithAmountConstraint>`.
    // Render the parallel arrays `reward_tokens` + `max_amounts` into a single
    // array of `{ asset, amount }` records. When `max_amounts` is missing or
    // shorter than `reward_tokens`, emit an `unknown` AmountConstraint so the
    // schema's required `amount` field stays populated.
    if let Some(reward_tokens) = &action.reward_tokens {
        let max_amounts = action.max_amounts.as_deref().unwrap_or(&[]);
        let unknown_amount = unknown_amount();
        let rendered = reward_tokens
            .iter()
            .enumerate()
            .map(|(i, asset)| {
                let mut entry = Map::new();
                entry.insert(ASSET.into(), asset_ref_json(asset)?);
                let amount_json = max_amounts
                    .get(i)
                    .map_or_else(|| unknown_amount.clone(), amount_constraint_json);
                entry.insert(AMOUNT.into(), amount_json);
                Ok(Value::Object(entry))
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        context.insert(REWARDS.into(), Value::Array(rendered));
    }
    Ok(Value::Object(context))
}

fn unknown_amount() -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), Value::from("unknown"));
    Value::Object(out)
}
