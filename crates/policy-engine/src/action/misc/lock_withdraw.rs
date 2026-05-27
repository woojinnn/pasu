//! Voting-escrow lock withdrawal (Aerodrome VotingEscrow.withdraw / Curve veCRV.withdraw).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRef, DecimalString};

/// Voting-escrow lock 만기 후 principal 회수.
///
/// - Aerodrome `VotingEscrow.withdraw(tokenId)` — NFT-bound, `token_id` set.
/// - Curve `veCRV.withdraw()` — account-bound, `token_id` omitted.
///
/// Lock-maturity invariant (`block.timestamp >= locked.end`) — pre-maturity
/// calls revert at the contract level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockWithdrawAction {
    /// `VotingEscrow` contract.
    pub voting_escrow: Address,
    /// veNFT token id — Aerodrome / Velodrome only (account-bound Curve veCRV omits this).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
    /// Token released from the lock (Aerodrome AERO / Curve CRV).
    pub asset: AssetRef,
    /// Withdrawn-token recipient — always msg.sender (root.from) for both
    /// Aerodrome and Curve.
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, assert_json_roundtrip, erc20};
    use serde_json::json;

    #[test]
    fn test_lock_withdraw_aerodrome_serde_roundtrip() {
        assert_json_roundtrip::<LockWithdrawAction>(json!({
            "votingEscrow": address(0x91),
            "tokenId": "42",
            "asset": erc20("AERO"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_lock_withdraw_curve_account_bound_serde_roundtrip() {
        // SX-3: Curve veCRV withdraw — tokenId omitted (account-bound).
        assert_json_roundtrip::<LockWithdrawAction>(json!({
            "votingEscrow": address(0x91),
            "asset": erc20("CRV"),
            "recipient": address(0x30)
        }));
    }
}
