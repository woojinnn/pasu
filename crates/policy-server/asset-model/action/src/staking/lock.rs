//! `LockAction` — create a vote-escrow lock (Curve veCRV `create_lock`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::U256;
use simulation_state::token::TokenRef;

use super::StakeVenue;

/// Lock a governance token for vote-escrow until `unlock_time`.
///
/// Models Curve `VotingEscrow.create_lock(uint256 _value, uint256 _unlock_time)`.
/// The locked token (CRV) is implied by the venue but carried explicitly for
/// policy clarity. `unlock_time` is a unix timestamp (seconds), U256 hex —
/// veCRV rounds it down to the nearest week and caps it at 4 years out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LockAction {
    /// Staking / vote-escrow venue (e.g. Curve `VotingEscrow`).
    pub venue: StakeVenue,
    /// Token being locked (e.g. CRV).
    pub token: TokenRef,
    /// Amount of `token` locked (`_value`, wei), U256 hex.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Lock expiry as a unix timestamp in seconds (`_unlock_time`), U256 hex.
    #[tsify(type = "string")]
    pub unlock_time: U256,
}
