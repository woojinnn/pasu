//! Repay action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, Validity};

use super::{AmountMode, MarketRef, RepayKind};

/// Repay debt in a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepayAction {
    /// Target lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Repayment asset.
    pub asset: AssetRef,
    /// Repayment amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Debt position owner.
    pub on_behalf: Address,
    /// Repayment funding source.
    pub repay_kind: RepayKind,
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
    fn test_repay_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<RepayAction>(json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "onBehalf": address(0x31),
            "repayKind": "debt_asset"
        }));
    }

    #[test]
    fn test_repay_action_serde_roundtrip_full() {
        assert_json_roundtrip::<RepayAction>(json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("unlimited", "0"),
            "amountMode": "shares",
            "onBehalf": address(0x31),
            "repayKind": "atoken_direct",
            "validity": validity()
        }));
    }
}
