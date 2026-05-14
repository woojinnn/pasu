//! Burn-liquidity-NFT action for concentrated-liquidity positions.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRef, AssetRefWithAmountConstraint, Validity};

use super::BurnKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Burn a concentrated-liquidity position NFT.
pub struct BurnLiquidityNftAction {
    /// NFT collection for the position.
    pub nft: AssetRef,
    /// Burn semantics.
    pub burn_kind: BurnKind,
    /// Output assets with amount constraints for auto-decrease burns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<AssetRefWithAmountConstraint>>,
    /// Recipient for auto-decrease burn outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::AmountKind;
    use crate::action::dex::test_support::{
        address, assert_roundtrip, asset_amount_pair, nft_position, validity,
    };

    #[test]
    fn test_burn_liquidity_nft_action_serde_roundtrip_minimal() {
        let action = BurnLiquidityNftAction {
            nft: nft_position(),
            burn_kind: BurnKind::EmptyOnly,
            outputs: None,
            recipient: None,
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_burn_liquidity_nft_action_serde_roundtrip_full() {
        let action = BurnLiquidityNftAction {
            nft: nft_position(),
            burn_kind: BurnKind::AutoDecrease,
            outputs: Some(asset_amount_pair(AmountKind::Min, AmountKind::Min)),
            recipient: Some(address("0x2222222222222222222222222222222222222222")),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }
}
