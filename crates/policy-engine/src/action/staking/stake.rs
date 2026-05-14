//! Stake action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

/// Stake a base asset and receive a staking receipt token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StakeAction {
    /// Asset being staked.
    pub token_in: AssetRef,
    /// Receipt token received from staking.
    pub receipt_token: AssetRef,
    /// Staked amount.
    pub amount_in: AmountConstraint,
    /// Expected or minimum receipt amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Recipient of the staking receipt.
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::staking::test_support::{address, amount, assert_json_roundtrip, erc20, native};
    use serde_json::json;

    #[test]
    fn test_stake_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<StakeAction>(json!({
            "tokenIn": native("ETH"),
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_stake_action_serde_roundtrip_full() {
        assert_json_roundtrip::<StakeAction>(json!({
            "tokenIn": native("ETH"),
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "recipient": address(0x30)
        }));
    }
}
