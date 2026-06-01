//! `ClaimVested` action — claims tokens that have vested from a launchpad allocation.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::position::PositionId;
use simulation_state::primitives::{Time, U256};
use simulation_state::LiveField;

/// Claims tokens that have vested from a launchpad allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimVestedAction {
    /// Position identifier — state §5 `LaunchpadAllocation` or `VestingSchedule`.
    pub position_id: PositionId,
    /// Amount to claim; `None` claims the maximum currently available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub amount: Option<U256>,
    /// Live on-chain inputs read at execution time.
    pub live_inputs: ClaimVestedLiveInputs,
}

/// Live-read inputs required to execute a `ClaimVestedAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimVestedLiveInputs {
    /// Amount currently claimable from the vesting schedule.
    #[tsify(type = "LiveField<string>")]
    pub claimable_now: LiveField<U256>,
    /// Next unlock as `(timestamp, amount)`, if any remain.
    #[tsify(type = "LiveField<[Time, string] | null>")]
    pub next_unlock: LiveField<Option<(Time, U256)>>,
}
