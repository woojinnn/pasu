//! VestingSchedule — 일반 vesting (option, team grant, OTC unlock 등) +
//! VestSchedule 공통 타입.
//!
//! LaunchpadAllocation 도 VestSchedule 을 재사용한다.

use serde::{Deserialize, Serialize};

use crate::primitives::{ProtocolRef, Time, U256};
use crate::token::TokenRef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VestCurve {
    /// 일정 비율 선형 unlock.
    Linear,
    /// 시점-수량 쌍의 step function.
    Stepped { points: Vec<(Time, U256)> },
    /// 위 두 가지에 안 맞는 vesting 곡선.
    Custom { description: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VestSchedule {
    pub start: Time,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cliff: Option<Time>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Time>,
    pub curve: VestCurve,
    pub total: U256,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VestingSchedule {
    pub granter: ProtocolRef,
    pub token: TokenRef,
    pub schedule: VestSchedule,
    pub claimed: U256,
    pub claimable_now: U256,
}
