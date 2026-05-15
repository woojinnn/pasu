//! Permit action.

use serde::{de, Deserialize, Deserializer, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, Validity};

use super::PermitKind;

/// Sign or relay a token permit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermitAction {
    /// Permit variant.
    pub permit_kind: PermitKind,
    /// Token authorized by the permit.
    pub token: AssetRef,
    /// Permit owner and signer.
    pub owner: Address,
    /// Spender for allowance-style permits, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spender: Option<Address>,
    /// Recipient for transfer-style permits, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Permitted amount or amount cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountConstraint>,
    /// Requested transfer amount, when distinct from the cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_amount: Option<AmountConstraint>,
    /// Operator for ERC-721 permit-for-all grants, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<Address>,
    /// Whether an ERC-721 permit-for-all grants or revokes operator access.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    /// Primary permit validity window.
    pub validity: Validity,
    /// Signature relay validity window, when separate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_validity: Option<Validity>,
}

impl<'de> Deserialize<'de> for PermitAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawPermitAction::deserialize(deserializer)?;

        Self::validate_required_fields(&raw).map_err(de::Error::custom)?;

        let owner = raw
            .owner
            .ok_or_else(|| de::Error::custom("owner is required for permit actions"))?;

        Ok(Self {
            permit_kind: raw.permit_kind,
            token: raw.token,
            owner,
            spender: raw.spender,
            recipient: raw.recipient,
            amount: raw.amount,
            requested_amount: raw.requested_amount,
            operator: raw.operator,
            approved: raw.approved,
            validity: raw.validity,
            signature_validity: raw.signature_validity,
        })
    }
}

impl PermitAction {
    #[allow(clippy::missing_const_for_fn)]
    fn validate_required_fields(raw: &RawPermitAction) -> Result<(), &'static str> {
        if raw.owner.is_none() {
            return Err("owner is required for permit actions");
        }

        if matches!(
            raw.permit_kind,
            PermitKind::Eip2612 | PermitKind::Permit2Single | PermitKind::Permit2Transfer
        ) && raw.amount.is_none()
        {
            return Err(
                "amount is required for eip2612, permit2_single, and permit2_transfer permits",
            );
        }

        if matches!(
            raw.permit_kind,
            PermitKind::Eip2612 | PermitKind::Permit2Single | PermitKind::Erc721Permit
        ) && raw.spender.is_none()
        {
            return Err(
                "spender is required for eip2612, permit2_single, and erc721_permit permits",
            );
        }

        if matches!(raw.permit_kind, PermitKind::Permit2Transfer) {
            if raw.recipient.is_none() {
                return Err("recipient is required for permit2_transfer permits");
            }

            if raw.requested_amount.is_none() {
                return Err("requestedAmount is required for permit2_transfer permits");
            }
        }

        if matches!(raw.permit_kind, PermitKind::Permit2Single) && raw.signature_validity.is_none()
        {
            return Err("signatureValidity is required for permit2_single permits");
        }

        if matches!(raw.permit_kind, PermitKind::Erc721Permit) && raw.token.token_id.is_none() {
            return Err("token.tokenId is required for erc721_permit permits");
        }

        if matches!(raw.permit_kind, PermitKind::Erc721PermitForAll) {
            if raw.operator.is_none() {
                return Err("operator is required for erc721_permit_for_all permits");
            }

            if raw.approved.is_none() {
                return Err("approved is required for erc721_permit_for_all permits");
            }
        }

        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPermitAction {
    permit_kind: PermitKind,
    token: AssetRef,
    owner: Option<Address>,
    spender: Option<Address>,
    recipient: Option<Address>,
    amount: Option<AmountConstraint>,
    requested_amount: Option<AmountConstraint>,
    operator: Option<Address>,
    approved: Option<bool>,
    validity: Validity,
    signature_validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{
        address, amount, assert_json_roundtrip, erc20, validity,
    };
    use serde_json::json;

    #[test]
    fn test_permit_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<PermitAction>(json!({
            "permitKind": "eip2612",
            "token": erc20("USDC"),
            "owner": address(0x52),
            "spender": address(0x53),
            "amount": amount("exact", "1000"),
            "validity": validity("signature-deadline")
        }));
    }

    #[test]
    fn test_permit_action_serde_roundtrip_full() {
        assert_json_roundtrip::<PermitAction>(json!({
            "permitKind": "permit2_transfer",
            "token": erc20("USDC"),
            "owner": address(0x52),
            "recipient": address(0x54),
            "amount": amount("max", "1000"),
            "requestedAmount": amount("exact", "900"),
            "validity": validity("signature-deadline")
        }));
    }

    #[test]
    fn test_permit_action_serde_roundtrip_erc721_permit_for_all() {
        assert_json_roundtrip::<PermitAction>(json!({
            "permitKind": "erc721_permit_for_all",
            "token": {
                "kind": "erc721",
                "address": address(0x11),
                "tokenId": "42",
                "symbol": "NFT"
            },
            "owner": address(0x52),
            "operator": address(0x55),
            "approved": true,
            "validity": validity("signature-deadline")
        }));
    }
}
