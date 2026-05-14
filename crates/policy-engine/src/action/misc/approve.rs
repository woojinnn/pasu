//! Approve action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, DecimalString, Validity};

use super::ApprovalKind;

/// Approve a spender for an amount-based token allowance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveAction {
    /// Token being approved.
    pub token: AssetRef,
    /// Spender receiving allowance.
    pub spender: Address,
    /// Human-readable spender label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spender_label: Option<String>,
    /// Approved amount.
    pub amount: AmountConstraint,
    /// Approval variant.
    pub approval_kind: ApprovalKind,
    /// Current allowance before this action, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_allowance: Option<DecimalString>,
    /// Approval validity window, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{
        address, amount, assert_json_roundtrip, erc20, validity,
    };
    use serde_json::json;

    #[test]
    fn test_approve_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<ApproveAction>(json!({
            "token": erc20("USDC"),
            "spender": address(0x40),
            "amount": amount("exact", "1000"),
            "approvalKind": "erc20"
        }));
    }

    #[test]
    fn test_approve_action_serde_roundtrip_full() {
        assert_json_roundtrip::<ApproveAction>(json!({
            "token": erc20("USDC"),
            "spender": address(0x40),
            "spenderLabel": "Known Router",
            "amount": amount("unlimited", "0"),
            "approvalKind": "permit2",
            "currentAllowance": "500",
            "validity": validity("grant-expiration")
        }));
    }
}
