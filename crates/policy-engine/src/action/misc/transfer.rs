//! Transfer action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, DecimalString};

/// Transfer a token directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferAction {
    /// Token being transferred.
    pub token: AssetRef,
    /// Account sending the token.
    pub from: Address,
    /// Account receiving the token.
    pub recipient: Address,
    /// Fungible or ERC-1155 amount, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountConstraint>,
    /// NFT or ERC-1155 token id, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{
        address, amount, assert_json_roundtrip, erc20, erc721,
    };
    use serde_json::json;

    #[test]
    fn test_transfer_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<TransferAction>(json!({
            "token": erc20("USDC"),
            "from": address(0x50),
            "recipient": address(0x51)
        }));
    }

    #[test]
    fn test_transfer_action_serde_roundtrip_full() {
        assert_json_roundtrip::<TransferAction>(json!({
            "token": erc721("NFT"),
            "from": address(0x50),
            "recipient": address(0x51),
            "amount": amount("exact", "1"),
            "tokenId": "42"
        }));
    }
}
