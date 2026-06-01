//! `UnwrapAction` — unwrap the non-rebasing wrapper back into the rebasing token.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::U256;
use policy_state::LiveField;

use super::StakingVenue;

/// Unwrap the non-rebasing wrapper (wstETH) back into the rebasing token (stETH).
///
/// Models wstETH `unwrap(uint256 _wstETHAmount)`: `amount` is the wrapper amount
/// burned; the returned rebasing-token amount is venue-derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct UnwrapAction {
    /// Liquid-staking venue.
    pub venue: StakingVenue,
    /// Amount of the wrapper to unwrap (wstETH units).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Live inputs fetched at simulation time.
    pub live_inputs: UnwrapLiveInputs,
}

/// Live-fetched inputs for an `UnwrapAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct UnwrapLiveInputs {
    /// stETH the unwrap is expected to return: wstETH `getStETHByWstETH(amount)`.
    /// Lets the user see the concrete rebasing-token amount behind the input.
    pub expected_steth: LiveField<U256>,
}
