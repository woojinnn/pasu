//! Request-unstake action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

use super::TicketRef;

/// Request delayed unstaking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestUnstakeAction {
    /// Receipt token being locked or burned.
    pub receipt_token: AssetRef,
    /// Asset expected after cooldown, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_out: Option<AssetRef>,
    /// Receipt token amount locked or burned.
    pub amount_in: AmountConstraint,
    /// Expected output amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Claim ticket produced by the request, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketRef>,
    /// Recipient of the claim right.
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::staking::test_support::{
        address, amount, assert_json_roundtrip, erc20, native, ticket,
    };
    use serde_json::json;

    #[test]
    fn test_request_unstake_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<RequestUnstakeAction>(json!({
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_request_unstake_action_serde_roundtrip_full() {
        assert_json_roundtrip::<RequestUnstakeAction>(json!({
            "receiptToken": erc20("stETH"),
            "tokenOut": native("ETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "ticket": ticket(),
            "recipient": address(0x30)
        }));
    }
}
