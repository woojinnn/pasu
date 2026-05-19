use crate::action::misc::ClaimRewardsAction;
use crate::action::AmountConstraint;
use crate::action::AssetRef;
use crate::context_keys::{
    FROM, NFT, RECIPIENT, REWARDS, SOURCE_ADDRESS, SOURCE_LABEL, TOKEN_ID,
};
use crate::lowering::common::asset::{asset_ref_json, asset_ref_with_amount_json};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "claim_rewards";

impl Lower for ClaimRewardsAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self, ctx)?))
    }
}

fn context(action: &ClaimRewardsAction, ctx: &LoweringCtx<'_>) -> Result<Value, LoweringError> {
    let mut out = Map::new();

    // sourceAddress: required by Cedar `ClaimRewardsContext.sourceAddress`.
    // Action schema allows source to be omitted (Option<SourceRef>); fall
    // back to the transaction target (ctx.to) when no explicit source was
    // emitted by the adapter — this matches the "Protocol::<to>" entity
    // already produced by LoweringCtx::request.
    let source_address = action
        .source
        .as_ref()
        .and_then(|s| s.address.as_ref())
        .map_or_else(|| ctx.to.to_string(), ToString::to_string);
    out.insert(SOURCE_ADDRESS.into(), Value::from(source_address));

    if let Some(label) = action.source.as_ref().and_then(|s| s.label.as_ref()) {
        out.insert(SOURCE_LABEL.into(), Value::from(label.clone()));
    }

    if let Some(nft) = &action.nft {
        out.insert(NFT.into(), asset_ref_json(nft)?);
    }
    if let Some(token_id) = &action.token_id {
        out.insert(TOKEN_ID.into(), Value::from(token_id.to_string()));
    }

    out.insert(FROM.into(), Value::from(action.from.to_string()));
    out.insert(RECIPIENT.into(), Value::from(action.recipient.to_string()));

    if let Some(rewards) = rewards_json(action)? {
        out.insert(REWARDS.into(), rewards);
    }

    Ok(Value::Object(out))
}

/// Cedar `ClaimRewardsContext.rewards?` is `Set<AssetRefWithAmountConstraint>`.
/// The action schema carries the parallel arrays `reward_tokens` +
/// `max_amounts` so the lowering joins them element-wise. The Set is emitted
/// only when both lists are present (otherwise Cedar would receive a
/// partially-populated set).
fn rewards_json(action: &ClaimRewardsAction) -> Result<Option<Value>, LoweringError> {
    let (Some(tokens), Some(amounts)) = (&action.reward_tokens, &action.max_amounts) else {
        return Ok(None);
    };
    if tokens.len() != amounts.len() {
        // Length mismatch — drop the rewards set rather than emit a
        // misaligned join. Cedar can still evaluate policies on the rest of
        // the context; a stronger fail would belong upstream in adapter
        // validation, not the lowering layer.
        return Ok(None);
    }
    let pairs: Vec<Value> = tokens
        .iter()
        .zip(amounts.iter())
        .map(|(asset, amount)| asset_ref_with_amount_pair(asset, amount))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(Value::Array(pairs)))
}

fn asset_ref_with_amount_pair(
    asset: &AssetRef,
    amount: &AmountConstraint,
) -> Result<Value, LoweringError> {
    asset_ref_with_amount_json(asset, amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::DecimalString;
    use crate::action::misc::SourceRef;
    use crate::action::{AmountKind, AssetKind};
    use std::str::FromStr;

    fn addr(byte: u8) -> crate::action::Address {
        crate::action::Address::from_str(&format!("0x{:040x}", byte)).expect("valid address")
    }

    fn ds(value: &str) -> DecimalString {
        DecimalString::from_str(value).expect("valid decimal")
    }

    fn ctx<'a>(
        from: &'a crate::action::Address,
        to: &'a crate::action::Address,
        value_wei: &'a DecimalString,
    ) -> LoweringCtx<'a> {
        LoweringCtx {
            from,
            to,
            value_wei,
            chain_id: 8453,
            block_timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn lower_claim_rewards_minimal_falls_back_to_protocol_target() {
        let action = ClaimRewardsAction {
            source: None,
            nft: None,
            token_id: None,
            from: addr(0x60),
            recipient: addr(0x61),
            reward_tokens: None,
            max_amounts: None,
        };
        let from = addr(0x60);
        let to = addr(0xAA);
        let value_wei = ds("0");
        let ctx = ctx(&from, &to, &value_wei);
        let req = action.build(&ctx).expect("build");
        let context = &req.context;
        // sourceAddress falls back to ctx.to when action.source is None.
        assert_eq!(context["sourceAddress"], Value::from(to.to_string()));
        assert_eq!(context["from"], Value::from(from.to_string()));
        assert_eq!(
            context["recipient"],
            Value::from(addr(0x61).to_string())
        );
        // Optional fields stay absent.
        assert!(context.get("sourceLabel").is_none());
        assert!(context.get("nft").is_none());
        assert!(context.get("tokenId").is_none());
        assert!(context.get("rewards").is_none());
    }

    #[test]
    fn lower_claim_rewards_full_emits_rewards_set() {
        let action = ClaimRewardsAction {
            source: Some(SourceRef {
                address: Some(addr(0xBB)),
                label: Some("Aerodrome Gauge".into()),
            }),
            nft: Some(AssetRef {
                kind: AssetKind::Erc721,
                address: Some(addr(0xCC)),
                token_id: Some(ds("42")),
                symbol: Some("veAERO".into()),
                decimals: None,
            }),
            token_id: Some(ds("42")),
            from: addr(0x60),
            recipient: addr(0x61),
            reward_tokens: Some(vec![
                AssetRef {
                    kind: AssetKind::Erc20,
                    address: Some(addr(0xDD)),
                    token_id: None,
                    symbol: Some("USDC".into()),
                    decimals: Some(6),
                },
                AssetRef {
                    kind: AssetKind::Erc20,
                    address: Some(addr(0xEE)),
                    token_id: None,
                    symbol: Some("AERO".into()),
                    decimals: Some(18),
                },
            ]),
            max_amounts: Some(vec![
                AmountConstraint {
                    kind: AmountKind::Max,
                    value: Some(ds("1000000")),
                },
                AmountConstraint {
                    kind: AmountKind::Max,
                    value: Some(ds("2000000000000000000")),
                },
            ]),
        };
        let from = addr(0x60);
        let to = addr(0xAA);
        let value_wei = ds("0");
        let ctx = ctx(&from, &to, &value_wei);
        let req = action.build(&ctx).expect("build");
        let context = &req.context;
        assert_eq!(context["sourceAddress"], Value::from(addr(0xBB).to_string()));
        assert_eq!(context["sourceLabel"], Value::from("Aerodrome Gauge"));
        assert_eq!(context["tokenId"], Value::from("42"));
        let rewards = context["rewards"].as_array().expect("rewards is array");
        assert_eq!(rewards.len(), 2);
        // Each entry has asset + amount sub-records.
        assert!(rewards[0]["asset"].is_object());
        assert!(rewards[0]["amount"].is_object());
    }

    #[test]
    fn lower_claim_rewards_length_mismatch_drops_rewards() {
        // Adapter bug → reward_tokens.len() != max_amounts.len(). Lowering
        // drops the rewards set so Cedar receives a structurally-valid
        // context instead of a partial Set.
        let action = ClaimRewardsAction {
            source: None,
            nft: None,
            token_id: None,
            from: addr(0x60),
            recipient: addr(0x61),
            reward_tokens: Some(vec![AssetRef {
                kind: AssetKind::Erc20,
                address: Some(addr(0xDD)),
                token_id: None,
                symbol: None,
                decimals: None,
            }]),
            max_amounts: Some(vec![]),
        };
        let from = addr(0x60);
        let to = addr(0xAA);
        let value_wei = ds("0");
        let ctx = ctx(&from, &to, &value_wei);
        let req = action.build(&ctx).expect("build");
        let context = &req.context;
        assert!(context.get("rewards").is_none());
    }
}
