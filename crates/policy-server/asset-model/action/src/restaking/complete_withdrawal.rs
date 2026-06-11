//! `CompleteWithdrawalAction` — complete a queued withdrawal, releasing funds.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::Address;

use super::RestakingVenue;

/// Complete a queued withdrawal.
///
/// Models `DelegationManager`
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
    /// `None` only when the flag could not be resolved (malformed/unresolved
    /// input). The batch `completeQueuedWithdrawals` decodes the per-withdrawal
    /// flag from the index-aligned `receiveAsTokens bool[]` via the `array_emit`
    /// `parallel_sources` mechanism, so it is normally `Some(_)` like the single.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub receive_as_tokens: Option<bool>,
}
