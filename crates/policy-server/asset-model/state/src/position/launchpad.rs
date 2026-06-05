//! `LaunchpadAllocation` combines subscription and vesting state.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::vesting::VestSchedule;
use crate::primitives::{ProtocolRef, U256};
use crate::token::TokenRef;

/// Subscription and vesting state for a launchpad or IDO allocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LaunchpadAllocation {
    /// Launchpad protocol that hosted this allocation.
    pub platform: ProtocolRef,
    /// Platform-local sale identifier.
    pub sale_id: String,
    /// Assets paid into the allocation.
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub paid: Vec<(TokenRef, U256)>,
    /// Asset allocated to the user.
    #[tsify(type = "[TokenRef, string]")]
    pub allocated: (TokenRef, U256),
    /// Vesting schedule for this allocation.
    pub vest: VestSchedule,
    /// Cumulative claimed amount.
    #[tsify(type = "string")]
    pub claimed: U256,
    /// Amount currently claimable.
    #[tsify(type = "string")]
    pub claimable_now: U256,
}
