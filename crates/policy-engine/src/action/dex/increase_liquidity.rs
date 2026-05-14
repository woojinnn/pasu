//! Increase-liquidity action for concentrated-liquidity positions.

use serde::{Deserialize, Serialize};

use crate::action::common::{AssetRef, AssetRefWithAmountConstraint, Validity};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Increase liquidity in an existing position NFT.
pub struct IncreaseLiquidityAction {
    /// NFT collection for the position.
    pub nft: AssetRef,
    /// Position token pair with amount constraints.
    pub inputs: Vec<AssetRefWithAmountConstraint>,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::AmountKind;
    use crate::action::dex::test_support::{
        assert_roundtrip, asset_amount_pair, nft_position, validity,
    };

    #[test]
    fn test_increase_liquidity_action_serde_roundtrip_minimal() {
        let action = IncreaseLiquidityAction {
            nft: nft_position(),
            inputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_increase_liquidity_action_serde_roundtrip_full() {
        let action = IncreaseLiquidityAction {
            nft: nft_position(),
            inputs: asset_amount_pair(AmountKind::Max, AmountKind::Max),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }
}
