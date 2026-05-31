//! `CompleteWithdrawalAction` — complete a queued withdrawal, releasing funds.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::Address;

use super::RestakingVenue;

/// Complete a queued withdrawal. Models DelegationManager
/// `completeQueuedWithdrawal(Withdrawal, address[] tokens, bool receiveAsTokens)`
/// (and the batch `completeQueuedWithdrawals`). Carries the user-legible subset
/// of the `Withdrawal` struct: `staker`, `withdrawer`, the `strategies`, and
/// whether funds are received as tokens (`true`) or re-deposited as shares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CompleteWithdrawalAction {
    /// Restaking venue.
    pub venue: RestakingVenue,
    /// Original staker of the withdrawal.
    #[tsify(type = "string")]
    pub staker: Address,
    /// Address completing the withdrawal and receiving the funds.
    #[tsify(type = "string")]
    pub withdrawer: Address,
    /// Strategies whose shares are being withdrawn.
    #[tsify(type = "string[]")]
    pub strategies: Vec<Address>,
    /// `true` = receive underlying tokens; `false` = re-deposit as shares.
    pub receive_as_tokens: bool,
}
