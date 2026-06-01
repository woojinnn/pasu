//! `LaunchpadAllocation` — Binance Launchpad / DAO Maker / Buidlpad 등의
//! 청약 + vest 통합 표현.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::vesting::VestSchedule;
use crate::primitives::{ProtocolRef, U256};
use crate::token::TokenRef;

/// Launchpad / IDO 의 청약 + vest 통합 (Binance Launchpad, DAO Maker 등).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LaunchpadAllocation {
    /// 본 청약을 호스팅한 launchpad 프로토콜.
    pub platform: ProtocolRef,
    /// 플랫폼-내 sale 식별자.
    pub sale_id: String,
    /// 청약에 사용한 자산들 ((USDC, 1000), (BNB, 5)).
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub paid: Vec<(TokenRef, U256)>,
    /// 받기로 한 자산.
    #[tsify(type = "[TokenRef, string]")]
    pub allocated: (TokenRef, U256),
    /// 본 allocation 의 vesting 일정.
    pub vest: VestSchedule,
    /// 누적으로 청구된 양.
    #[tsify(type = "string")]
    pub claimed: U256,
    /// 지금 청구 가능한 양.
    #[tsify(type = "string")]
    pub claimable_now: U256,
}
