//! `Cooldown` action — start the unstake cooldown window.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::StakeVenue;

/// Whether a partial-cooldown `amount` is denominated in vault **shares** or in
/// the underlying **assets** (Ethena `cooldownShares` vs `cooldownAssets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum CooldownDenomination {
    /// `amount` is a count of vault shares (e.g. sUSDe `cooldownShares(shares)`).
    Shares,
    /// `amount` is a count of the underlying asset (e.g. sUSDe `cooldownAssets(assets)`).
    Assets,
}

/// Start the cooldown window that must elapse before a staked position can be
/// withdrawn. Two shapes:
///
/// - **Whole-balance** (Aave `StakedTokenV3.cooldown()`): no `amount` — marks the
///   caller's entire staked balance as cooling down. Umbrella's cooldown carries
///   the `account` whose balance cools down.
/// - **Partial / amount-specified** (Ethena `StakedUSDeV2.cooldownShares(shares)`
///   / `cooldownAssets(assets)`): `amount` is the quantity moved into cooldown,
///   `denomination` says whether it is shares or underlying assets. The funds are
///   locked in a silo for the venue's cooldown window before `unstake`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CooldownAction {
    /// Staking venue (safety module, Umbrella stake token, or Ethena sUSDe).
    pub venue: StakeVenue,
    /// Account whose stake is cooling down (Umbrella). Omitted ⇒ submitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub account: Option<Address>,
    /// Quantity moved into cooldown (Ethena `cooldownShares`/`cooldownAssets`),
    /// U256 hex. Omitted ⇒ whole staked balance (Aave `cooldown()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub amount: Option<U256>,
    /// Whether `amount` is denominated in vault shares or underlying assets.
    /// Present iff `amount` is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub denomination: Option<CooldownDenomination>,
}
