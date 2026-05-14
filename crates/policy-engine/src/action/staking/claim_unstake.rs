//! Claim-unstake action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

use super::TicketRef;

/// Claim a completed unstake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimUnstakeAction {
    /// Asset received from the unstake claim.
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
    use crate::action::staking::test_support::{
        address, amount, assert_json_roundtrip, native, ticket,
    };
    use serde_json::json;

    #[test]
    fn test_claim_unstake_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<ClaimUnstakeAction>(json!({
            "tokenOut": native("ETH"),
            "ticket": {},
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_claim_unstake_action_serde_roundtrip_full() {
        assert_json_roundtrip::<ClaimUnstakeAction>(json!({
            "tokenOut": native("ETH"),
            "amountOut": amount("exact", "999"),
            "ticket": ticket(),
            "recipient": address(0x30)
        }));
    }
}
