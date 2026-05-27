//! Position — 토큰 형태가 아닌 protocol-tracked 권리/상태. spec §5.

use serde::{Deserialize, Serialize};

pub mod airdrop;
pub mod launchpad;
pub mod lending;
pub mod perp;
pub mod vesting;

pub use airdrop::{AirdropClaim, ClaimStatus, MerkleProof};
pub use launchpad::LaunchpadAllocation;
pub use lending::{EModeCategory, LendingAccount};
pub use perp::{MarginMode, PerpPosition, PerpSide};
pub use vesting::{VestCurve, VestSchedule, VestingSchedule};

use crate::live_field::DataSource;
use crate::primitives::{ChainId, ProtocolRef, Time};

/// PositionId — protocol 이 부여한 안정 id 또는 우리가 생성한 string.
pub type PositionId = String;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub id: PositionId,
    pub protocol: ProtocolRef,
    /// off-chain venue 의 경우 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainId>,
    pub kind: PositionKind,
    pub primitives_synced_at: Time,
    pub primitives_source: DataSource,
}

/// Position 의 variant. 토큰 형태가 아닌 권리만 여기로 모음.
/// (concentrated LP NFT 같은 토큰화된 포지션은 tokens 에 LpShare kind 로 들어감)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PositionKind {
    LendingAccount(LendingAccount),
    PerpPosition(PerpPosition),
    AirdropClaim(AirdropClaim),
    LaunchpadAllocation(LaunchpadAllocation),
    VestingSchedule(VestingSchedule),
}
