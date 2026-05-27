//! Voting-escrow lock creation (Aerodrome VotingEscrow.createLock).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRefWithAmountConstraint, DecimalString};

/// Voting-escrow lock creation — Aerodrome `VotingEscrow.createLock` and equivalents.
///
/// Equivalents include Curve veCRV. Locks `asset.amount` of `asset.asset` into
/// `voting_escrow`; lock period is given as either a relative
/// `lock_duration_sec` or an absolute `unlock_time` (mutually exclusive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockCreateAction {
    /// `VotingEscrow` contract.
    pub voting_escrow: Address,
    /// Asset being locked (e.g. AERO ERC20) with the lock amount.
    pub asset: AssetRefWithAmountConstraint,
    /// Lock duration in seconds (relative). Aerodrome `createLock(value, lockDuration)`.
    /// Mutually exclusive with `unlock_time` — exactly one is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_duration_sec: Option<DecimalString>,
    /// Absolute unlock timestamp (epoch seconds). Curve veCRV
    /// `create_lock(_value, _unlock_time)`. Mutually exclusive with `lock_duration_sec`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlock_time: Option<DecimalString>,
    /// Recipient of the veNFT (default = tx sender, override via createLockFor).
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, amount, assert_json_roundtrip, erc20};
    use serde_json::json;

    #[test]
    fn test_lock_create_serde_roundtrip() {
        assert_json_roundtrip::<LockCreateAction>(json!({
            "votingEscrow": address(0x91),
            "asset": {
                "asset": erc20("AERO"),
                "amount": amount("exact", "1000000000000000000")
            },
            "lockDurationSec": "126144000",
            "recipient": address(0x30)
        }));
    }
}
