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
    /// Collateral asset deposited in the same call. crvUSD `create_loan` /
    /// `borrow_more` pull collateral alongside the borrow. `None` for
    /// borrow-only flows (Aave / Compound / Morpho borrow takes no collateral).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collateral_asset: Option<AssetRef>,
    /// Collateral amount deposited. Parallel to `collateral_asset` — both
    /// present or both absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collateral_amount: Option<AmountConstraint>,
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
        address, amount, assert_json_roundtrip, erc20, market, validity,
    };
    use serde_json::json;

    #[test]
    fn test_borrow_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<BorrowAction>(json!({
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30),
            "onBehalf": address(0x31)
        }));
    }

    #[test]
    fn test_borrow_action_serde_roundtrip_full() {
        assert_json_roundtrip::<BorrowAction>(json!({
            "market": market(),
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "amountMode": "shares",
            "recipient": address(0x30),
            "onBehalf": address(0x31),
            "validity": validity()
        }));
    }
}
