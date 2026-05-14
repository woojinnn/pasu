//! Supply action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, Validity};

use super::{AmountMode, MarketRef};

/// Supply assets to a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyAction {
    /// Target lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Asset being supplied.
    pub asset: AssetRef,
    /// Supply amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Account that receives the supply position.
    pub recipient: Address,
    /// Account that provides the asset, when different or explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// Validity window, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::test_support::{
        address, amount, asset, assert_json_roundtrip, market, validity,
    };
    use serde_json::json;

    #[test]
    fn test_supply_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<SupplyAction>(json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_supply_action_serde_roundtrip_full() {
        assert_json_roundtrip::<SupplyAction>(json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "amountMode": "shares",
            "recipient": address(0x30),
            "from": address(0x31),
            "validity": validity()
        }));
    }
}
