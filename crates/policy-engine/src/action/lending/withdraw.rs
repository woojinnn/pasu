//! Withdraw action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

use super::{AmountMode, MarketRef};

/// Withdraw supplied assets from a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawAction {
    /// Source lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Asset being withdrawn.
    pub asset: AssetRef,
    /// Withdrawal amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Account receiving withdrawn assets.
    pub recipient: Address,
    /// Supply position owner, when different or explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf: Option<Address>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::test_support::{
        address, amount, asset, assert_json_roundtrip, market,
    };
    use serde_json::json;

    #[test]
    fn test_withdraw_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<WithdrawAction>(json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_withdraw_action_serde_roundtrip_full() {
        assert_json_roundtrip::<WithdrawAction>(json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("unlimited", "0"),
            "amountMode": "shares",
            "recipient": address(0x30),
            "onBehalf": address(0x31)
        }));
    }
}
