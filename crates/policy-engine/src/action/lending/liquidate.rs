//! Liquidate action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

use super::{LiquidateMode, LiquidationKind, MarketRef};

/// Liquidate an unhealthy lending position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidateAction {
    /// Lending market where liquidation occurs, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Borrower being liquidated.
    pub borrower: Address,
    /// Collateral asset being seized, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collateral_asset: Option<AssetRef>,
    /// Debt asset being repaid or absorbed.
    pub debt_asset: AssetRef,
    /// Debt amount to cover, when debt-side input is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debt_to_cover: Option<AmountConstraint>,
    /// Collateral amount to seize, when collateral-side input is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seized_collateral_amount: Option<AmountConstraint>,
    /// Liquidation mechanism.
    pub liquidation_kind: LiquidationKind,
    /// Liquidation input mode, when explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidate_mode: Option<LiquidateMode>,
    /// Recipient of seized assets, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Whether Aave collateral is received as aToken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receive_a_token: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::test_support::{
        address, amount, asset, assert_json_roundtrip, market,
    };
    use serde_json::json;

    #[test]
    fn test_liquidate_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<LiquidateAction>(json!({
            "borrower": address(0x40),
            "debtAsset": asset("USDC"),
            "liquidationKind": "pool_share"
        }));
    }

    #[test]
    fn test_liquidate_action_serde_roundtrip_full() {
        assert_json_roundtrip::<LiquidateAction>(json!({
            "market": market(),
            "borrower": address(0x40),
            "collateralAsset": asset("WETH"),
            "debtAsset": asset("USDC"),
            "debtToCover": amount("exact", "1000"),
            "seizedCollateralAmount": amount("estimated", "1"),
            "liquidationKind": "socializable",
            "liquidateMode": "seize",
            "recipient": address(0x30),
            "receiveAToken": true
        }));
    }
}
