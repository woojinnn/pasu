//! `Cooldown` action — start the safety-module unstake cooldown window.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::StakeVenue;

/// Start the cooldown window that must elapse before a safety-module stake can
/// be redeemed (Aave `StakedTokenV3.cooldown()`). Takes no arguments — it marks
/// the caller's whole staked balance as cooling down; the redeemed amount is
/// chosen later at `redeem` time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CooldownAction {
    /// Staking venue (`StakeVenue::AaveSafetyModule { chain, module }`).
    pub venue: StakeVenue,
}
