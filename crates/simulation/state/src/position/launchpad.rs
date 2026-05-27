//! LaunchpadAllocation — Binance Launchpad / DAO Maker / Buidlpad 등의
//! 청약 + vest 통합 표현.

use serde::{Deserialize, Serialize};

use super::vesting::VestSchedule;
use crate::primitives::{ProtocolRef, U256};
use crate::token::TokenRef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchpadAllocation {
    pub platform: ProtocolRef,
    pub sale_id: String,
    /// 청약에 사용한 자산들 ((USDC, 1000), (BNB, 5)).
    pub paid: Vec<(TokenRef, U256)>,
    /// 받기로 한 자산.
    pub allocated: (TokenRef, U256),
    pub vest: VestSchedule,
    pub claimed: U256,
    pub claimable_now: U256,
}
