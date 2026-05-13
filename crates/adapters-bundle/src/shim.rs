//! Lossy bridge from new `ActionEnvelope` shapes back to legacy adapter actions.
//! Used by Phase 5.F transitional code to keep legacy consumers working
//! while the migration is in progress.

use alloy_primitives::U256;
use policy_engine::action::misc::PermitKind;
use policy_engine::action::{
    Action, ActionEnvelope, AmountConstraint, AmountKind, AssetKind, AssetRef,
};
use policy_engine::{
    Address, DexAction, DexFacts, DexTrace, Eip2612Action, LegacyAction, Permit2Action,
    Permit2Approval, Permit2PermitKind, Token,
};

const UINT160_MAX_DEC: &str = "1461501637330902918203684832716283019655932542975";
const UINT256_MAX_DEC: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

fn to_token(asset: &AssetRef) -> Option<Token> {
    let address = asset.address.as_ref()?;

    Some(Token {
        chain_id: asset.chain_id,
        address: Address::new(&address.to_string()).ok()?,
        symbol: asset.symbol.clone().unwrap_or_default(),
        decimals: u32::from(asset.decimals.unwrap_or(0)),
        is_native: matches!(asset.kind, AssetKind::Native),
    })
}

fn to_address(address: &policy_engine::action::Address) -> Option<Address> {
    Address::new(&address.to_string()).ok()
}

fn amount_to_decimal(amount: &AmountConstraint, unlimited_sentinel: &str) -> Option<String> {
    if matches!(&amount.kind, AmountKind::Unlimited) {
        return Some(unlimited_sentinel.to_owned());
    }

    let value = amount.value.as_ref()?.to_string();
    Some(U256::from_str_radix(&value, 10).ok()?.to_string())
}

fn decimal_to_u64(value: &policy_engine::action::DecimalString) -> Option<u64> {
    value.to_string().parse().ok()
}

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
    signer: &Address,
    chain_id: u64,
) -> Option<LegacyAction> {
    let Action::Permit(permit) = &envelope.action else {
        return None;
    };

    let owner = to_address(&permit.owner)?;
    if owner.as_str() != signer.as_str() {
        return None;
    }

    let token = to_token(&permit.token)?;
    let spender = to_address(permit.spender.as_ref()?)?;
    let deadline = decimal_to_u64(&permit.validity.expires_at)?;

    // The new envelope does not retain the original EIP-712 domain, nonce, or
    // decoded valuation facts. This lossy legacy shim defaults chain/domain
    // chain to the caller chain id, EIP-2612 verifying contract to the token,
    // Permit2 verifying contract to the canonical Permit2 deployment, nonce to
    // "0", nonce_valid to true, witness_present to false, and USD valuation to
    // None.
    match &permit.permit_kind {
        PermitKind::Eip2612 => {
            let value = amount_to_decimal(&permit.amount, UINT256_MAX_DEC)?;
            let is_unlimited =
                matches!(&permit.amount.kind, AmountKind::Unlimited) || value == UINT256_MAX_DEC;

            Some(LegacyAction::Eip2612(Eip2612Action {
                signer: signer.clone(),
                owner: signer.clone(),
                chain_id,
                domain_chain_id: chain_id,
                verifying_contract: token.address.clone(),
                primary_type: "Permit".to_owned(),
                spender,
                token,
                is_unlimited,
                nonce_valid: true,
                value,
                deadline,
                nonce: "0".to_owned(),
                total_approved_usd: None,
            }))
        }
        PermitKind::Permit2Single | PermitKind::Permit2Transfer => {
            let permit_kind = match &permit.permit_kind {
                PermitKind::Permit2Single => Permit2PermitKind::PermitSingle,
                PermitKind::Permit2Transfer => Permit2PermitKind::PermitTransferFrom,
                PermitKind::Eip2612 => return None,
            };
            let unlimited_sentinel = match permit_kind {
                Permit2PermitKind::PermitSingle => UINT160_MAX_DEC,
                Permit2PermitKind::PermitTransferFrom => UINT256_MAX_DEC,
                Permit2PermitKind::PermitBatch
                | Permit2PermitKind::PermitBatchTransferFrom
                | Permit2PermitKind::PermitWitnessTransferFrom
                | Permit2PermitKind::PermitBatchWitnessTransferFrom => return None,
            };
            let amount = amount_to_decimal(&permit.amount, unlimited_sentinel)?;
            let sig_deadline = permit
                .signature_validity
                .as_ref()
                .map_or(Some(deadline), |validity| {
                    decimal_to_u64(&validity.expires_at)
                })?;
            let nonce = "0".to_owned();
            let approval = Permit2Approval {
                token: token.clone(),
                amount: amount.clone(),
                expiration: deadline,
                nonce: nonce.clone(),
            };
            let is_unlimited = matches!(&permit.amount.kind, AmountKind::Unlimited)
                || approval.amount == UINT160_MAX_DEC;

            Some(LegacyAction::Permit2(Permit2Action {
                signer: signer.clone(),
                chain_id,
                domain_chain_id: chain_id,
                verifying_contract: Address::new(crate::permit2::PERMIT2_ADDRESS).ok()?,
                primary_type: permit_kind.as_str().to_owned(),
                permit_kind,
                spender,
                token,
                amount,
                expiration: deadline,
                sig_deadline,
                nonce,
                approvals: vec![approval],
                is_unlimited,
                nonce_valid: true,
                witness_present: false,
                total_approved_usd: None,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_engine::action::common::{AmountConstraint, AssetRef, DecimalString};
    use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use policy_engine::action::envelope::{Action as NewAction, ActionEnvelope, Category};
    use policy_engine::action::misc::{PermitAction, PermitKind};
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

    fn permit_token() -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            chain_id: 1,
            address: Some(
                policy_engine::action::common::Address::from_str(
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                )
                .unwrap(),
            ),
            symbol: Some("USDC".into()),
            decimals: Some(6),
        }
    }

    fn permit_envelope(kind: PermitKind) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Misc,
            action: NewAction::Permit(PermitAction {
                permit_kind: kind,
                token: permit_token(),
                owner: policy_engine::action::common::Address::from_str(
                    "0x0000000000000000000000000000000000000001",
                )
                .unwrap(),
                spender: Some(
                    policy_engine::action::common::Address::from_str(
                        "0x0000000000000000000000000000000000000002",
                    )
                    .unwrap(),
                ),
                spender_label: None,
                recipient: None,
                amount: AmountConstraint {
                    kind: AmountKind::Exact,
                    value: Some(DecimalString::from_str("1000000").unwrap()),
                },
                requested_amount: None,
                validity: policy_engine::action::common::Validity {
                    expires_at: DecimalString::from_str("1700000000").unwrap(),
                    source: policy_engine::action::common::ValiditySource::SignatureDeadline,
                },
                signature_validity: None,
            }),
        }
    }

    #[test]
    fn eip2612_permit_envelope_lowers_to_legacy_eip2612() {
        let signer = addr("0x0000000000000000000000000000000000000001");
        let envelope = permit_envelope(PermitKind::Eip2612);

        let result = permit_envelope_to_legacy_sig(&envelope, &signer, 1);

        assert!(matches!(result, Some(LegacyAction::Eip2612(_))));
    }

    #[test]
    fn permit2_single_envelope_lowers_to_legacy_permit2() {
        let signer = addr("0x0000000000000000000000000000000000000001");
        let envelope = permit_envelope(PermitKind::Permit2Single);

        let result = permit_envelope_to_legacy_sig(&envelope, &signer, 1);

        assert!(matches!(result, Some(LegacyAction::Permit2(_))));
    }

    #[test]
    fn permit2_transfer_envelope_lowers_to_legacy_permit2() {
        let signer = addr("0x0000000000000000000000000000000000000001");
        let envelope = permit_envelope(PermitKind::Permit2Transfer);

        let result = permit_envelope_to_legacy_sig(&envelope, &signer, 1);

        assert!(matches!(result, Some(LegacyAction::Permit2(_))));
    }
}
