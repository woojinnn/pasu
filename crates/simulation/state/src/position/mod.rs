//! Position — 토큰 형태가 아닌 protocol-tracked 권리/상태. spec §5.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// 에어드랍 클레임 권리 (`AirdropClaim`, `ClaimStatus`, `MerkleProof`).
pub mod airdrop;
/// Launchpad 청약 + vest 통합 (`LaunchpadAllocation`).
pub mod launchpad;
/// Lending market 계정 집계 (`LendingAccount`, `EModeCategory`).
pub mod lending;
/// 무기한 선물 포지션 (`PerpPosition`, `PerpSide`, `MarginMode`).
pub mod perp;
/// 일반 vesting 일정 (`VestingSchedule`, `VestSchedule`, `VestCurve`).
pub mod vesting;

pub use airdrop::{AirdropClaim, ClaimStatus, MerkleProof};
pub use launchpad::LaunchpadAllocation;
pub use lending::{EModeCategory, LendingAccount};
pub use perp::{MarginMode, PerpPosition, PerpSide};
pub use vesting::{VestCurve, VestSchedule, VestingSchedule};

use crate::live_field::DataSource;
use crate::primitives::{ChainId, ProtocolRef, Time};

/// `PositionId` — protocol 이 부여한 안정 id 또는 우리가 생성한 string.
pub type PositionId = String;

/// 토큰 형태가 아닌 protocol-tracked 권리/상태. wallet 의 `positions` list 요소.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Position {
    /// 본 포지션 식별자 (프로토콜 부여 또는 우리 생성 string).
    pub id: PositionId,
    /// 포지션이 속한 프로토콜.
    pub protocol: ProtocolRef,
    /// off-chain venue 의 경우 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub chain: Option<ChainId>,
    /// 포지션의 sub-kind 와 그 본체 데이터.
    pub kind: PositionKind,
    /// 본 포지션의 primitive 필드가 마지막으로 sync 된 시각.
    pub primitives_synced_at: Time,
    /// primitive 필드의 출처.
    pub primitives_source: DataSource,
}

/// Position 의 variant. 토큰 형태가 아닌 권리만 여기로 모음.
/// (concentrated LP NFT 같은 토큰화된 포지션은 tokens 에 `LpShare` kind 로 들어감)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PositionKind {
    /// Lending market 한 계정의 집계 (HF / LTV / emode 등).
    LendingAccount(LendingAccount),
    /// Hyperliquid / GMX / dYdX 등 perp 오픈 포지션.
    PerpPosition(PerpPosition),
    /// 에어드랍 클레임 권리.
    AirdropClaim(AirdropClaim),
    /// Launchpad 청약 + vest.
    LaunchpadAllocation(LaunchpadAllocation),
    /// 일반 vesting 일정 (option, team grant 등).
    VestingSchedule(VestingSchedule),
}
