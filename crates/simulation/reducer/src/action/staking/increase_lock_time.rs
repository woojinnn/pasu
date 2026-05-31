//! `IncreaseLockTimeAction` — extend the unlock time of an existing lock.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::U256;

use super::StakeVenue;

/// Extend the unlock time of an existing vote-escrow lock, without adding tokens.
///
/// Models Curve `VotingEscrow.increase_unlock_time(uint256 _unlock_time)`.
/// `unlock_time` is a unix timestamp (seconds), U256 hex — veCRV rounds it down
/// to the nearest week and caps it at 4 years out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct IncreaseLockTimeAction {
    /// Staking / vote-escrow venue (e.g. Curve `VotingEscrow`).
    pub venue: StakeVenue,
    /// New lock expiry as a unix timestamp in seconds (`_unlock_time`), U256 hex.
    #[tsify(type = "string")]
    pub unlock_time: U256,
}
