//! AirdropClaim — 에어드랍 클레임 권리.

use serde::{Deserialize, Serialize};

use crate::primitives::{ProtocolRef, Time, U256};
use crate::token::TokenRef;

/// 머클 클레임 proof. depth 보존을 위해 hex 문자열 배열로.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf_index: u64,
    /// 32-byte hash 들의 hex string ("0x..").
    pub siblings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Eligible,
    Claimable,
    Claimed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirdropClaim {
    pub source: ProtocolRef,
    pub claimable: TokenRef,
    pub amount: U256,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<MerkleProof>,
    /// 클레임 가능 기간 (start, end).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_window: Option<(Time, Time)>,
    pub status: ClaimStatus,
}
