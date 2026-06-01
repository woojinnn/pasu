//! `GaugeWithdrawAction` — unstake LP tokens from a Curve liquidity gauge.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::U256;

use super::StakeVenue;

/// Unstake LP tokens from a Curve liquidity gauge.
///
/// Models Curve gauge `withdraw(uint256 _value)` and
/// `withdraw(uint256 _value, bool _claim_rewards)`. The withdrawn LP token is the
/// gauge's underlying — identified by the gauge venue. The optional
/// `_claim_rewards` flag is an execution convenience, not represented.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GaugeWithdrawAction {
    /// Staking venue (Curve liquidity gauge).
    pub venue: StakeVenue,
    /// Amount of LP unstaked (`_value`), U256 hex.
    #[tsify(type = "string")]
    pub amount: U256,
}
