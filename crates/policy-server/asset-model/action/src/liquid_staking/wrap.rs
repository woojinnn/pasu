//! `WrapAction` — wrap the rebasing staking token into its non-rebasing wrapper.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::U256;
use policy_state::LiveField;

use super::StakingVenue;

/// Wrap the rebasing staking token (stETH) into its wrapper (wstETH).
///
/// Models wstETH `wrap(uint256 _stETHAmount)`: `amount` is the rebasing-token
/// amount supplied; the minted wrapper amount is venue-derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WrapAction {
    /// Liquid-staking venue.
    pub venue: StakingVenue,
    /// Amount of the rebasing token to wrap (stETH units).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Live inputs fetched at simulation time.
    pub live_inputs: WrapLiveInputs,
}

/// Live-fetched inputs for a `WrapAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WrapLiveInputs {
    /// wstETH the wrap is expected to mint: wstETH `getWstETHByStETH(amount)`.
    /// Lets the user see the concrete wrapper amount behind the abstract input.
    pub expected_wsteth: LiveField<U256>,
}
