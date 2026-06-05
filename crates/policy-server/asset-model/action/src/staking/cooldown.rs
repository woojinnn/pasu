//! `Cooldown` action — start the safety-module unstake cooldown window.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::Address;

use super::StakeVenue;

/// Start the cooldown window that must elapse before a safety-module stake can
/// be redeemed (Aave `StakedTokenV3.cooldown()`). The legacy safety module takes
/// no arguments (marks the caller's whole staked balance as cooling down);
/// Umbrella's cooldown carries the `account` whose balance cools down.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CooldownAction {
    /// Staking venue (safety module or Umbrella stake token).
    pub venue: StakeVenue,
    /// Account whose stake is cooling down (Umbrella). Omitted ⇒ submitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub account: Option<Address>,
}
