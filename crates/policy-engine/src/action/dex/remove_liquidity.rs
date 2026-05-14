//! Remove-liquidity action for fungible LP pools.

use serde::{Deserialize, Serialize};

use crate::action::common::{
    Address, AmountConstraint, AssetRef, AssetRefWithAmountConstraint, Validity,
};

use super::{PoolRef, RemoveLiquidityExitMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Remove liquidity from a fungible LP pool.
pub struct RemoveLiquidityAction {
    /// Withdrawal mode.
    pub exit_mode: RemoveLiquidityExitMode,
    /// Source pool, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolRef>,
    /// LP token being burned, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_token: Option<AssetRef>,
    /// LP burn amount constraint, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_burn_amount: Option<AmountConstraint>,
    /// Underlying pool assets with amount constraints.
    pub outputs: Vec<AssetRefWithAmountConstraint>,
    /// Recipient of the withdrawn assets.
    pub recipient: Address,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::AmountKind;
    use crate::action::dex::test_support::{
        address, amount, assert_roundtrip, asset_amount_pair, erc20, pool, validity,
    };

    #[test]
    fn test_remove_liquidity_action_serde_roundtrip_minimal() {
        let action = RemoveLiquidityAction {
            exit_mode: RemoveLiquidityExitMode::Proportional,
            pool: None,
            lp_token: None,
            lp_burn_amount: None,
            outputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_remove_liquidity_action_serde_roundtrip_full() {
        let action = RemoveLiquidityAction {
            exit_mode: RemoveLiquidityExitMode::SingleAsset,
            pool: Some(pool()),
            lp_token: Some(erc20(
                "0x3333333333333333333333333333333333333333",
                "UNI-V2",
                18,
            )),
            lp_burn_amount: Some(amount(AmountKind::Exact, "100000000000000000")),
            outputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }
}
