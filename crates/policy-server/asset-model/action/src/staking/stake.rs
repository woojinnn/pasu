//! `Stake` action — stake into an Aave safety module (`stake` /
//! `stakeWithPermit`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::StakeVenue;

/// Stake the underlying token into a safety module and receive its staked
/// derivative (Aave `StakedTokenV3.stake(to, amount)` /
/// `stakeWithPermit(amount, …)`). The staked token (AAVE / GHO) is implied by
/// the venue's module address; `recipient` is the share receiver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct StakeAction {
    /// Staking venue (`StakeVenue::AaveSafetyModule { chain, module }`).
    pub venue: StakeVenue,
    /// Amount of the underlying token staked (`amount`, wei), U256 hex.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Recipient of the staked-derivative shares (`to`). Omitted ⇒ submitter
    /// (e.g. `stakeWithPermit`, which mints to `msg.sender`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub recipient: Option<Address>,
}
