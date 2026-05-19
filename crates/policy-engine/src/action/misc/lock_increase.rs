//! Voting-escrow lock modification (Aerodrome VotingEscrow.increaseAmount / .increaseUnlockTime).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, DecimalString};

/// Modify an existing voting-escrow lock — either add to the principal
/// (`amount`) or extend the unlock time (`unlock_time`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockIncreaseAction {
    /// VotingEscrow contract.
    pub voting_escrow: Address,
    /// veNFT token id being modified.
    pub token_id: DecimalString,
    /// Subkind discriminator.
    pub kind: LockIncreaseKind,
    /// Additional locked amount (when `kind == amount`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_amount: Option<AmountConstraint>,
    /// New lock duration in seconds (when `kind == unlock_time`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_lock_duration_sec: Option<DecimalString>,
}

/// Distinguishes the two voting-escrow lock-increase paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockIncreaseKind {
    /// Add additional locked tokens to an existing lock (increaseAmount).
    Amount,
    /// Extend the unlock timestamp of an existing lock (increaseUnlockTime).
    UnlockTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, amount, assert_json_roundtrip};
    use serde_json::json;

    #[test]
    fn test_lock_increase_amount_serde_roundtrip() {
        assert_json_roundtrip::<LockIncreaseAction>(json!({
            "votingEscrow": address(0x91),
            "tokenId": "42",
            "kind": "amount",
            "additionalAmount": amount("exact", "500000000000000000")
        }));
    }

    #[test]
    fn test_lock_increase_unlock_time_serde_roundtrip() {
        assert_json_roundtrip::<LockIncreaseAction>(json!({
            "votingEscrow": address(0x91),
            "tokenId": "42",
            "kind": "unlock_time",
            "newLockDurationSec": "126144000"
        }));
    }
}
