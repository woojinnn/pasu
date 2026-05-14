//! Permit action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, Validity};

use super::PermitKind;

/// Sign or relay a token permit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Human-readable spender label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spender_label: Option<String>,
    /// Recipient for transfer-style permits, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Permitted amount or amount cap.
    pub amount: AmountConstraint,
    /// Requested transfer amount, when distinct from the cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_amount: Option<AmountConstraint>,
    /// Primary permit validity window.
    pub validity: Validity,
    /// Signature relay validity window, when separate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_validity: Option<Validity>,
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
            "spender": address(0x53),
            "spenderLabel": "Known Spender",
            "recipient": address(0x54),
            "amount": amount("max", "1000"),
            "requestedAmount": amount("exact", "900"),
            "validity": validity("signature-deadline"),
            "signatureValidity": validity("signature-deadline")
        }));
    }
}
