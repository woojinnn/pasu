//! `VestingSchedule` — generic vesting (options, team grants, OTC unlocks, etc.)
//! plus the shared `VestSchedule` type.
//! `LaunchpadAllocation` also reuses `VestSchedule`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{ProtocolRef, Time, U256};
use crate::token::TokenRef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Shape of the unlock curve that governs how vested tokens become available over time.
pub enum VestCurve {
    /// Linear unlock at a constant rate over the vesting period.
    Linear,
    /// Step function defined by explicit (time, cumulative-amount) points.
    Stepped {
        /// Ordered unlock checkpoints as (timestamp, cumulative unlocked amount) pairs.
        #[tsify(type = "Array<[Time, string]>")]
        points: Vec<(Time, U256)>,
    },
    /// Vesting curve that does not fit the linear or stepped shapes, described free-form.
    Custom {
        /// Human-readable description of the custom unlock behavior.
        description: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// Common parameters describing a single vesting schedule, shared across vesting and allocation types.
pub struct VestSchedule {
    /// Timestamp at which vesting begins.
    pub start: Time,
    /// Optional cliff timestamp before which nothing unlocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub cliff: Option<Time>,
    /// Optional timestamp at which vesting completes; `None` for open-ended schedules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub end: Option<Time>,
    /// Shape of the unlock curve applied between `start` and `end`.
    pub curve: VestCurve,
    /// Total token amount covered by this schedule (raw on-chain units).
    #[tsify(type = "string")]
    pub total: U256,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// A vesting grant held by a position, tracking its schedule and claim progress.
pub struct VestingSchedule {
    /// Protocol or entity that granted the vesting allocation.
    pub granter: ProtocolRef,
    /// Token being vested.
    pub token: TokenRef,
    /// Schedule parameters governing how the grant unlocks.
    pub schedule: VestSchedule,
    /// Amount already claimed so far (raw on-chain units).
    #[tsify(type = "string")]
    pub claimed: U256,
    /// Amount currently claimable at the present time (raw on-chain units).
    #[tsify(type = "string")]
    pub claimable_now: U256,
}
