//! `GaugeDepositAction` — stake LP tokens into a Curve liquidity gauge.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};

use super::StakeVenue;

/// Stake LP tokens into a Curve liquidity gauge to earn rewards.
///
/// Models Curve gauge `deposit(uint256 _value)`, `deposit(uint256, address _addr)`
/// and `deposit(uint256, address _addr, bool _claim_rewards)`. The staked LP token
/// is the gauge's underlying — identified by the gauge venue, so it is not carried
/// separately. `on_behalf_of` is the credited account (`_addr`); omitted ⇒ submitter.
/// The optional `_claim_rewards` flag is an execution convenience, not represented.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GaugeDepositAction {
    /// Staking venue (Curve liquidity gauge).
    pub venue: StakeVenue,
    /// Amount of LP staked (`_value`), U256 hex.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Account credited with the stake (`_addr`); omitted ⇒ submitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub on_behalf_of: Option<Address>,
}
