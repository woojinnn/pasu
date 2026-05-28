//! VestingSchedule — 일반 vesting (option, team grant, OTC unlock 등) +
//! VestSchedule 공통 타입.
//!
//! LaunchpadAllocation 도 VestSchedule 을 재사용한다.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{ProtocolRef, Time, U256};
use crate::token::TokenRef;

/// vesting unlock 곡선 형태.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VestCurve {
    /// 일정 비율 선형 unlock.
    Linear,
    /// 시점-수량 쌍의 step function.
    Stepped {
        /// (시각, 누적 unlock 양) pair list.
        #[tsify(type = "Array<[Time, string]>")]
        points: Vec<(Time, U256)>,
    },
    /// 위 두 가지에 안 맞는 vesting 곡선.
    Custom {
        /// 사람이 읽을 곡선 설명 (display 용).
        description: String,
    },
}

/// vesting 일정의 메타 (start / cliff / end / curve / total).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct VestSchedule {
    /// vesting 시작 시각.
    pub start: Time,
    /// cliff 시각. 없는 vesting 은 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub cliff: Option<Time>,
    /// vesting 종료 시각. 무기한 vesting 은 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub end: Option<Time>,
    /// unlock 곡선.
    pub curve: VestCurve,
    /// 본 일정으로 vest 되는 총량.
    #[tsify(type = "string")]
    pub total: U256,
}

/// 일반 vesting position (option / team grant / OTC unlock 등).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct VestingSchedule {
    /// vest 를 부여한 entity (프로토콜 / 단체).
    pub granter: ProtocolRef,
    /// vest 대상 토큰.
    pub token: TokenRef,
    /// vesting 일정 본체.
    pub schedule: VestSchedule,
    /// 누적으로 청구된 양.
    #[tsify(type = "string")]
    pub claimed: U256,
    /// 지금 청구 가능한 양.
    #[tsify(type = "string")]
    pub claimable_now: U256,
}
