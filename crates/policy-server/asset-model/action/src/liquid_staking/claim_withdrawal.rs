//! `ClaimWithdrawalAction` — redeem finalized withdrawal request(s) for ETH.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::StakingVenue;

/// Claim finalized withdrawal request(s) — redeems queued request NFTs for ETH.
///
/// Models Lido `claimWithdrawal(uint256)`, `claimWithdrawals(uint256[],uint256[])`
/// and `claimWithdrawalsTo(uint256[],uint256[],address)`. `recipient` is the
/// explicit payout target when present (else the submitter). Finalization hints
/// are an execution detail and are not represented.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimWithdrawalAction {
    /// Liquid-staking venue.
    pub venue: StakingVenue,
    /// Withdrawal request ids being claimed.
    #[tsify(type = "string[]")]
    pub request_ids: Vec<U256>,
    /// Explicit payout recipient; defaults to the submitter when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub recipient: Option<Address>,
}
