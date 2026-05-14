//! Claim-restake-withdrawal action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};
use crate::action::staking::TicketRef;

/// Claim a completed restaking withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRestakeWithdrawalAction {
    /// Asset received from the withdrawal claim.
    pub token_out: AssetRef,
    /// Claimed amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Claim ticket being consumed.
    pub ticket: TicketRef,
    /// Recipient of claimed assets.
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::restaking::test_support::{
        address, amount, assert_json_roundtrip, native, ticket,
    };
    use serde_json::json;

    #[test]
    fn test_claim_restake_withdrawal_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<ClaimRestakeWithdrawalAction>(json!({
            "tokenOut": native("ETH"),
            "ticket": {},
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_claim_restake_withdrawal_action_serde_roundtrip_full() {
        assert_json_roundtrip::<ClaimRestakeWithdrawalAction>(json!({
            "tokenOut": native("ETH"),
            "amountOut": amount("exact", "999"),
            "ticket": ticket(),
            "recipient": address(0x30)
        }));
    }
}
