//! `VoteForGaugeAction` — allocate vote-escrow weight to a gauge
//! (Curve `GaugeController.vote_for_gauge_weights`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::StakeVenue;

/// Allocate a share of the submitter's vote-escrow weight to a gauge.
///
/// Models Curve `GaugeController.vote_for_gauge_weights(address _gauge_addr,
/// uint256 _user_weight)`. `weight_bp` is the weight in basis points (0–10000;
/// 10000 = 100% of the voter's veCRV weight), U256 hex. Moves no funds — it
/// reallocates the caller's existing vote-escrow power.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct VoteForGaugeAction {
    /// Staking venue (e.g. Curve `GaugeController`).
    pub venue: StakeVenue,
    /// Gauge receiving the weight allocation (`_gauge_addr`).
    #[tsify(type = "string")]
    pub gauge: Address,
    /// Weight in basis points 0–10000 (`_user_weight`), U256 hex.
    #[tsify(type = "string")]
    pub weight_bp: U256,
}
