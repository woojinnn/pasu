//! `StakeAction` — stake native ETH into a liquid-staking protocol (Lido `submit`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::StakingVenue;

/// Stake native ETH and receive a liquid-staking token.
///
/// Models Lido `submit(address _referral)` (payable): `amount` is the ETH sent
/// (`msg.value`); the received stETH is implied by the venue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct StakeAction {
    /// Liquid-staking venue (e.g. `Lido` on Ethereum).
    pub venue: StakingVenue,
    /// Amount of native ETH staked (`msg.value`, wei).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Optional referral address (Lido `_referral`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub referral: Option<Address>,
}
