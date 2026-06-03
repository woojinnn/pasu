//! `Redeem` action — withdraw the underlying from a safety module post-cooldown.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::StakeVenue;

/// Redeem a safety-module stake: burn the staked derivative and withdraw the
/// underlying token to `recipient` (Aave `StakedTokenV3.redeem(to, amount)`).
/// Requires the cooldown window to have elapsed. The underlying token (AAVE /
/// GHO) is implied by the venue's module address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RedeemAction {
    /// Staking venue (`StakeVenue::AaveSafetyModule { chain, module }`).
    pub venue: StakeVenue,
    /// Amount of staked-derivative shares redeemed (`amount`, wei), U256 hex.
    /// `type(uint256).max` redeems the full balance.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Recipient of the withdrawn underlying (`to`). Omitted ⇒ submitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub recipient: Option<Address>,
}
