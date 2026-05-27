//! Voting-escrow lock modification (Aerodrome VotingEscrow.increaseAmount / .increaseUnlockTime
//! / Curve veCRV.increase_amount / .increase_unlock_time / .deposit_for).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, DecimalString};

/// Modify an existing voting-escrow lock — either add to the principal
/// (`amount`) or extend the unlock time (`unlock_time`).
///
/// Three protocol shapes are supported via optional fields:
/// - Aerodrome `increaseAmount(tokenId, value)` / `increaseUnlockTime(tokenId, lockDuration)`
///   — NFT-bound: `token_id` is required, `new_lock_duration_sec` is relative seconds.
/// - Curve `increase_amount(value)` / `increase_unlock_time(unlock_time)`
///   — account-bound: `token_id` omitted, `new_unlock_time` is absolute Unix timestamp.
/// - Curve `deposit_for(addr, value)` — account-bound third-party deposit: `recipient` set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockIncreaseAction {
    /// `VotingEscrow` contract.
    pub voting_escrow: Address,
    /// veNFT token id — Aerodrome / Velodrome only (account-bound Curve veCRV omits this).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
    /// Subkind discriminator.
    pub kind: LockIncreaseKind,
    /// Additional locked amount (when `kind == amount`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_amount: Option<AmountConstraint>,
    /// New lock duration in seconds (relative, Aerodrome). Mutually exclusive
    /// with `new_unlock_time`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_lock_duration_sec: Option<DecimalString>,
    /// Absolute unlock timestamp (Curve veCRV `_unlock_time`). Mutually
    /// exclusive with `new_lock_duration_sec`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_unlock_time: Option<DecimalString>,
    /// Third-party lock owner (Curve veCRV `deposit_for(_addr, _value)`'s
    /// `_addr`). Omitted = caller's own lock (`increaseAmount` direct call).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
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

    #[test]
    fn test_lock_increase_amount_curve_account_bound_serde_roundtrip() {
        // SX-1: Curve veCRV increase_amount — tokenId omitted (account-bound).
        assert_json_roundtrip::<LockIncreaseAction>(json!({
            "votingEscrow": address(0x91),
            "kind": "amount",
            "additionalAmount": amount("exact", "500000000000000000")
        }));
    }

    #[test]
    fn test_lock_increase_unlock_time_curve_absolute_serde_roundtrip() {
        // SX-2: Curve veCRV increase_unlock_time — absolute timestamp + tokenId omitted.
        assert_json_roundtrip::<LockIncreaseAction>(json!({
            "votingEscrow": address(0x91),
            "kind": "unlock_time",
            "newUnlockTime": "1893456000"
        }));
    }

    #[test]
    fn test_lock_increase_deposit_for_serde_roundtrip() {
        // SX-4: Curve veCRV deposit_for — recipient set (third-party lock).
        assert_json_roundtrip::<LockIncreaseAction>(json!({
            "votingEscrow": address(0x91),
            "kind": "amount",
            "additionalAmount": amount("exact", "1000000000000000000"),
            "recipient": address(0x42)
        }));
    }
}
