//! Restake action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

use super::StrategyRef;

/// Restake an asset into a strategy or vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestakeAction {
    /// Asset being restaked.
    pub token_in: AssetRef,
    /// Receipt token received from restaking, when one is minted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_token: Option<AssetRef>,
    /// Restaked amount.
    pub amount_in: AmountConstraint,
    /// Expected or minimum receipt amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Strategy or vault receiving the asset, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<StrategyRef>,
    /// Recipient of shares or receipt tokens.
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::restaking::test_support::{
        address, amount, assert_json_roundtrip, erc20, strategy,
    };
    use serde_json::json;

    #[test]
    fn test_restake_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<RestakeAction>(json!({
            "tokenIn": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_restake_action_serde_roundtrip_full() {
        assert_json_roundtrip::<RestakeAction>(json!({
            "tokenIn": erc20("stETH"),
            "receiptToken": erc20("ezETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "strategy": strategy(),
            "recipient": address(0x30)
        }));
    }
}
