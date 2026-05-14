//! Request-restake-withdrawal action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};
use crate::action::staking::TicketRef;

use super::StrategyRef;

/// Request a delayed restaking withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestRestakeWithdrawalAction {
    /// Asset expected after escrow, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_out: Option<AssetRef>,
    /// Receipt token being burned, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_token: Option<AssetRef>,
    /// Amount locked or burned.
    pub amount_in: AmountConstraint,
    /// Expected output amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Strategy or vault being unwound, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<StrategyRef>,
    /// Claim ticket produced by the request, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketRef>,
    /// Recipient of the claim right.
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::restaking::test_support::{
        address, amount, assert_json_roundtrip, erc20, native, strategy, ticket,
    };
    use serde_json::json;

    #[test]
    fn test_request_restake_withdrawal_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<RequestRestakeWithdrawalAction>(json!({
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_request_restake_withdrawal_action_serde_roundtrip_full() {
        assert_json_roundtrip::<RequestRestakeWithdrawalAction>(json!({
            "tokenOut": native("ETH"),
            "receiptToken": erc20("ezETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "strategy": strategy(),
            "ticket": ticket(),
            "recipient": address(0x30)
        }));
    }
}
