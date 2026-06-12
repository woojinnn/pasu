//! Position — protocol-tracked rights/state that are not held in token form. spec §5.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

pub mod airdrop;
pub mod hyperliquid;
pub mod launchpad;
pub mod lending;
pub mod perp;
pub mod vesting;

pub use airdrop::{AirdropClaim, ClaimStatus, MerkleProof};
pub use hyperliquid::{
    CoreFresh, EquityAnchor, HlAccount, HlAgentApproval, HlBorrowLendAccount, HlBorrowLendBalance,
    HlBorrowLendTokenState, HlFillSummary, HlLeverageSetting, HlOpenOrder, HlPosition,
    HlSpotBalance, HlStakingAccount, HlStakingDelegation, HlVaultEquity, LongtailFresh,
};
pub use launchpad::LaunchpadAllocation;
pub use lending::{EModeCategory, LendingAccount};
pub use perp::{MarginMode, PerpPosition, PerpSide};
pub use vesting::{VestCurve, VestSchedule, VestingSchedule};

use crate::live_field::DataSource;
use crate::primitives::{ChainId, ProtocolRef, Time};

/// `PositionId` — a stable id assigned by the protocol, or a string we generate.
pub type PositionId = String;

/// A single protocol-tracked position (a non-token right/state) held by an account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Position {
    /// Stable identifier for this position.
    pub id: PositionId,
    /// Protocol this position belongs to.
    pub protocol: ProtocolRef,
    /// Chain the position lives on; `None` for off-chain venues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub chain: Option<ChainId>,
    /// Kind-specific payload describing what this position is.
    pub kind: PositionKind,
    /// Timestamp at which the position primitives were last synced.
    pub primitives_synced_at: Time,
    /// Origin of the synced primitives (e.g. RPC, indexer).
    pub primitives_source: DataSource,
}

/// Variants of a [`Position`]. Only non-token rights are collected here.
/// (Tokenized positions such as a concentrated LP NFT live in `tokens` under the `LpShare` kind.)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PositionKind {
    /// Aggregated account state in a single lending market (HF, LTV, emode, isolation).
    LendingAccount(LendingAccount),
    /// An open perpetual-futures position on a derivatives venue.
    PerpPosition(PerpPosition),
    /// A claimable airdrop allocation.
    AirdropClaim(AirdropClaim),
    /// An allocation acquired through a launchpad sale.
    LaunchpadAllocation(LaunchpadAllocation),
    /// A token vesting schedule (locked/unlocking allocation over time).
    VestingSchedule(VestingSchedule),
    /// A wallet's Hyperliquid L1 account state (off-chain ledger).
    HyperliquidAccount(HlAccount),
}
