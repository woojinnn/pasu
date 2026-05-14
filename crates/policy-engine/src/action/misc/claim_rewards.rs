//! Claim-rewards action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, DecimalString};

use super::SourceRef;

/// Claim accrued reward tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRewardsAction {
    /// Reward source contract, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
    /// Position NFT collection, when rewards are NFT-position based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nft: Option<AssetRef>,
    /// Position NFT token id, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
    /// Account whose rewards are claimed.
    pub from: Address,
    /// Account receiving claimed rewards.
    pub recipient: Address,
    /// Reward token list, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_tokens: Option<Vec<AssetRef>>,
    /// Maximum claim amounts matching `reward_tokens`, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_amounts: Option<Vec<AmountConstraint>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{
        address, amount, assert_json_roundtrip, erc20, erc721, source,
    };
    use serde_json::json;

    #[test]
    fn test_claim_rewards_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<ClaimRewardsAction>(json!({
            "from": address(0x60),
            "recipient": address(0x61)
        }));
    }

    #[test]
    fn test_claim_rewards_action_serde_roundtrip_full() {
        assert_json_roundtrip::<ClaimRewardsAction>(json!({
            "source": source(),
            "nft": erc721("POSITION"),
            "tokenId": "42",
            "from": address(0x60),
            "recipient": address(0x61),
            "rewardTokens": [erc20("USDC"), erc20("WETH")],
            "maxAmounts": [amount("max", "1000"), amount("max", "2")]
        }));
    }
}
