//! Borrow action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, Validity};

use super::{AmountMode, MarketRef};

/// Borrow assets from a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BorrowAction {
    /// Source lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Borrowed asset.
    pub asset: AssetRef,
    /// Borrow amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Account receiving borrowed assets.
    pub recipient: Address,
    /// Debt position owner.
    pub on_behalf: Address,
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
    fn test_borrow_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<BorrowAction>(json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30),
            "onBehalf": address(0x31)
        }));
    }

    #[test]
    fn test_borrow_action_serde_roundtrip_full() {
        assert_json_roundtrip::<BorrowAction>(json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "amountMode": "shares",
            "recipient": address(0x30),
            "onBehalf": address(0x31),
            "validity": validity()
        }));
    }
}
