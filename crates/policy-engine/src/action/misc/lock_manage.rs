//! Voting-escrow lock management (Aerodrome VotingEscrow.merge / .split).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, DecimalString};

/// Merge two voting-escrow positions, or split one position in two.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockManageAction {
    /// VotingEscrow contract.
    pub voting_escrow: Address,
    /// Subkind discriminator.
    pub kind: LockManageKind,
    /// Source veNFT token id (consumed on merge / split).
    pub from_token_id: DecimalString,
    /// Destination veNFT token id when merging.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_token_id: Option<DecimalString>,
    /// Split ratio (when `kind == split`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_ratio: Option<DecimalString>,
}

/// Distinguishes the two voting-escrow lock-manage paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockManageKind {
    /// Merge `from` into `to` (sum amount, take latest unlock time).
    Merge,
    /// Split `from` into a new position based on `splitRatio`.
    Split,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, assert_json_roundtrip};
    use serde_json::json;

    #[test]
    fn test_lock_manage_merge_serde_roundtrip() {
        assert_json_roundtrip::<LockManageAction>(json!({
            "votingEscrow": address(0x91),
            "kind": "merge",
            "fromTokenId": "1",
            "toTokenId": "2"
        }));
    }

    #[test]
    fn test_lock_manage_split_serde_roundtrip() {
        assert_json_roundtrip::<LockManageAction>(json!({
            "votingEscrow": address(0x91),
            "kind": "split",
            "fromTokenId": "1",
            "splitRatio": "500000000000000000"
        }));
    }
}
