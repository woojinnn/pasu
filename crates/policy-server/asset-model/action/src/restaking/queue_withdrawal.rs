//! `QueueWithdrawalAction` — queue a withdrawal of staked restaking shares.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::RestakingVenue;

/// Queue a withdrawal.
///
/// Models one `DelegationManager`
/// `queueWithdrawals(QueuedWithdrawalParams[])` element
/// `{address[] strategies, uint256[] depositShares, address withdrawer}`
/// (post-ELIP-002 shape). `deposit_shares` is the protocol's internal share
/// unit (NOT underlying-token units); converting it needs a per-strategy
/// share→underlying view (array-shaped) — enrichment deferred.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct QueueWithdrawalAction {
    /// Restaking venue.
    pub venue: RestakingVenue,
    /// Strategies whose shares are being withdrawn.
    #[tsify(type = "string[]")]
    pub strategies: Vec<Address>,
    /// Per-strategy share amounts (internal share unit).
    #[tsify(type = "string[]")]
    pub deposit_shares: Vec<U256>,
    /// Address that will complete the withdrawal and receive the funds.
    #[tsify(type = "string")]
    pub withdrawer: Address,
}
