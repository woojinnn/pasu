//! Flash-loan action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

use super::{FlashLoanKind, MarketRef};

/// Borrow assets and repay them in the same transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashLoanAction {
    /// Pool or market issuing the flash loan, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<MarketRef>,
    /// Borrowed assets.
    pub assets: Vec<AssetRef>,
    /// Borrowed amounts matching `assets`.
    pub amounts: Vec<AmountConstraint>,
    /// Callback receiver contract.
    pub receiver: Address,
    /// Account that receives debt if the loan is converted to debt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf: Option<Address>,
    /// Flash loan variant.
    pub flash_loan_kind: FlashLoanKind,
    /// Flash loan fee, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<AmountConstraint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::test_support::{
        address, amount, asset, assert_json_roundtrip, market,
    };
    use serde_json::json;

    #[test]
    fn test_flash_loan_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<FlashLoanAction>(json!({
            "assets": [asset("USDC")],
            "amounts": [amount("exact", "1000")],
            "receiver": address(0x50),
            "flashLoanKind": "simple"
        }));
    }

    #[test]
    fn test_flash_loan_action_serde_roundtrip_full() {
        assert_json_roundtrip::<FlashLoanAction>(json!({
            "pool": market(),
            "assets": [asset("USDC"), asset("WETH")],
            "amounts": [amount("exact", "1000"), amount("exact", "2")],
            "receiver": address(0x50),
            "onBehalf": address(0x31),
            "flashLoanKind": "multi",
            "fee": amount("exact", "5")
        }));
    }
}
