//! Decrease-liquidity action for concentrated-liquidity positions.

use serde::{Deserialize, Serialize};

use crate::action::common::{
    Address, AmountConstraint, AssetRef, AssetRefWithAmountConstraint, Validity,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Decrease liquidity in an existing position NFT.
pub struct DecreaseLiquidityAction {
    /// NFT collection for the position.
    pub nft: AssetRef,
    /// Internal liquidity amount to remove.
    pub liquidity_delta: AmountConstraint,
    /// Output assets with amount constraints.
    pub outputs: Vec<AssetRefWithAmountConstraint>,
    /// Recipient of withdrawn assets, when the protocol sends them immediately.
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
        address, amount, assert_roundtrip, asset_amount_pair, nft_position, validity,
    };

    #[test]
    fn test_decrease_liquidity_action_serde_roundtrip_minimal() {
        let action = DecreaseLiquidityAction {
            nft: nft_position(),
            liquidity_delta: amount(AmountKind::Exact, "100000000000000000"),
            outputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
            recipient: None,
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_decrease_liquidity_action_serde_roundtrip_full() {
        let action = DecreaseLiquidityAction {
            nft: nft_position(),
            liquidity_delta: amount(AmountKind::Exact, "100000000000000000"),
            outputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
            recipient: Some(address("0x2222222222222222222222222222222222222222")),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }
}
