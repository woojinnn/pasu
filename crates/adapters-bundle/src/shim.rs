//! Lossy bridge from new `ActionEnvelope` shapes back to legacy adapter actions.
//! Used by Phase 5.F transitional code to keep legacy consumers working
//! while the migration is in progress.

use policy_engine::action::misc::PermitKind;
use policy_engine::action::{Action, ActionEnvelope, AmountKind, AssetKind, AssetRef};
use policy_engine::{Address, DexAction, DexFacts, DexTrace, LegacyAction, Token};

/// Convert a `Action::Swap` envelope into a legacy `LegacyAction::Dex`.
///
/// Returns `None` for any other variant. Lossy: `oracle_requirements` and
/// trace are defaulted; `protocol_ids` is a singleton "unknown".
#[must_use]
pub fn swap_envelope_to_legacy_dex(
    envelope: &ActionEnvelope,
    actor: &Address,
    target: &Address,
    value_wei: &str,
) -> Option<LegacyAction> {
    let Action::Swap(swap) = &envelope.action else {
        return None;
    };

    let to_token = |asset: &AssetRef| -> Option<Token> {
        let address = asset.address.as_ref()?;

        Some(Token {
            chain_id: asset.chain_id,
            address: Address::new(&address.to_string()).ok()?,
            symbol: asset.symbol.clone().unwrap_or_default(),
            decimals: u32::from(asset.decimals.unwrap_or(0)),
            is_native: matches!(asset.kind, AssetKind::Native),
        })
    };

    let input_tokens = to_token(&swap.token_in).into_iter().collect::<Vec<_>>();
    let output_tokens = to_token(&swap.token_out).into_iter().collect::<Vec<_>>();

    let has_zero_min_output = matches!(swap.amount_out.kind, AmountKind::Min)
        && swap
            .amount_out
            .value
            .as_ref()
            .is_some_and(|d| d.to_string() == "0");

    let has_external_recipient = swap.recipient.to_string() != actor.as_str();

    Some(LegacyAction::Dex(DexAction {
        actor: actor.clone(),
        target: target.clone(),
        value_wei: value_wei.to_owned(),
        facts: DexFacts {
            protocol_ids: vec!["unknown".to_owned()],
            input_tokens,
            output_tokens,
            has_zero_min_output,
            has_external_recipient,
            max_fee_bps: swap.fee_bps,
            ..Default::default()
        },
        oracle_requirements: Vec::new(),
        trace: DexTrace::default(),
    }))
}

/// Convert a `Action::Permit` envelope into the legacy signature action.
///
/// Returns `LegacyAction::Eip2612` or `LegacyAction::Permit2` depending on the
/// `PermitKind`. Returns `None` for any other variant.
#[allow(clippy::missing_const_for_fn)]
#[must_use]
pub fn permit_envelope_to_legacy_sig(
    envelope: &ActionEnvelope,
    _signer: &Address,
    _chain_id: u64,
) -> Option<LegacyAction> {
    let Action::Permit(permit) = &envelope.action else {
        return None;
    };

    // intentional stub - returns None for now
    match &permit.permit_kind {
        PermitKind::Eip2612 | PermitKind::Permit2Single | PermitKind::Permit2Transfer => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_engine::action::common::{AmountConstraint, AssetRef, DecimalString};
    use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use policy_engine::action::envelope::{Action as NewAction, ActionEnvelope, Category};
    use std::str::FromStr;

    fn addr(s: &str) -> Address {
        Address::new(s).unwrap()
    }

    #[test]
    fn swap_envelope_lowers_to_legacy_dex() {
        let actor = addr("0x0000000000000000000000000000000000000001");
        let target = addr("0x0000000000000000000000000000000000000002");
        let token_in = AssetRef {
            kind: AssetKind::Erc20,
            chain_id: 1,
            address: Some(
                policy_engine::action::common::Address::from_str(
                    "0xdac17f958d2ee523a2206206994597c13d831ec7",
                )
                .unwrap(),
            ),
            symbol: Some("USDT".into()),
            decimals: Some(6),
        };
        let token_out = AssetRef {
            kind: AssetKind::Erc20,
            chain_id: 1,
            address: Some(
                policy_engine::action::common::Address::from_str(
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                )
                .unwrap(),
            ),
            symbol: Some("WETH".into()),
            decimals: Some(18),
        };
        let swap = SwapAction {
            mode: SwapMode::ExactIn,
            token_in,
            token_out,
            amount_in: AmountConstraint {
                kind: AmountKind::Exact,
                value: Some(DecimalString::from_str("1000").unwrap()),
            },
            amount_out: AmountConstraint {
                kind: AmountKind::Min,
                value: Some(DecimalString::from_str("0").unwrap()),
            },
            recipient: policy_engine::action::common::Address::from_str(
                "0x0000000000000000000000000000000000000003",
            )
            .unwrap(),
            validity: None,
            fee_bps: Some(30),
            enrichment: SwapEnrichment::default(),
        };
        let envelope = ActionEnvelope {
            category: Category::Dex,
            action: NewAction::Swap(swap),
        };

        let legacy = swap_envelope_to_legacy_dex(&envelope, &actor, &target, "0").unwrap();
        let LegacyAction::Dex(dex) = legacy else {
            panic!("expected Dex")
        };
        assert_eq!(dex.facts.input_tokens.len(), 1);
        assert_eq!(dex.facts.output_tokens.len(), 1);
        assert!(dex.facts.has_zero_min_output);
        assert!(dex.facts.has_external_recipient);
        assert_eq!(dex.facts.max_fee_bps, Some(30));
    }

    #[test]
    fn non_swap_returns_none() {
        let actor = addr("0x0000000000000000000000000000000000000001");
        let target = addr("0x0000000000000000000000000000000000000002");
        let envelope_json = serde_json::json!({
            "category": "misc",
            "action": "approve",
            "fields": {
                "token": { "kind": "erc20", "chainId": 1, "address": "0xdac17f958d2ee523a2206206994597c13d831ec7" },
                "spender": "0x0000000000000000000000000000000000000003",
                "amount": { "kind": "unlimited" },
                "approvalKind": "erc20"
            }
        });
        let envelope: ActionEnvelope = serde_json::from_value(envelope_json).unwrap();
        assert!(swap_envelope_to_legacy_dex(&envelope, &actor, &target, "0").is_none());
    }
}
