//! Voting-escrow lock creation (Aerodrome VotingEscrow.createLock).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, DecimalString};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockCreateAction {
    /// VotingEscrow contract.
    pub voting_escrow: Address,
    /// Asset being locked (e.g. AERO ERC20).
    pub asset: AssetRef,
    /// Lock amount.
    pub amount: AmountConstraint,
    /// Lock duration in seconds.
    pub lock_duration_sec: DecimalString,
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
            "asset": erc20("AERO"),
            "amount": amount("exact", "1000000000000000000"),
            "lockDurationSec": "126144000",
            "recipient": address(0x30)
        }));
    }
}
