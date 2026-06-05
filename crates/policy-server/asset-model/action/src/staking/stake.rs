//! `Stake` action — stake into an Aave safety module (`stake` /
//! `stakeWithPermit`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::StakeVenue;

/// Stake the underlying token into a safety module and receive its staked
/// derivative (Aave `StakedTokenV3.stake(to, amount)` /
/// `stakeWithPermit(amount, …)`). The staked token (AAVE / GHO) is implied by
/// the venue's module address; `recipient` is the share receiver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct StakeAction {
    /// Staking venue (safety module or Umbrella stake token).
    pub venue: StakeVenue,
    /// Staked underlying token. Present for Umbrella (the deposited edge/asset
    /// token is explicit in calldata); omitted for the legacy safety module
    /// where the staked token (AAVE / GHO) is implied by the venue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub asset: Option<TokenRef>,
    /// Amount of the underlying token staked (`amount`, wei), U256 hex.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Account staked on behalf of (Umbrella `onBehalfOf`). Omitted ⇒ submitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub on_behalf_of: Option<Address>,
    /// Recipient of the staked-derivative shares (`to`). Omitted ⇒ submitter
    /// (e.g. `stakeWithPermit`, which mints to `msg.sender`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub recipient: Option<Address>,
}
