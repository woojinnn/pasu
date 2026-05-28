//! AirdropClaim — 에어드랍 클레임 권리.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{ProtocolRef, Time, U256};
use crate::token::TokenRef;

/// 머클 클레임 proof. depth 보존을 위해 hex 문자열 배열로.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MerkleProof {
    /// 머클 트리 내 leaf 의 index.
    pub leaf_index: u64,
    /// 32-byte hash 들의 hex string ("0x..").
    pub siblings: Vec<String>,
}

/// 에어드랍 클레임 lifecycle 상태.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    /// 자격은 있으나 아직 claim window 가 열리지 않음.
    Eligible,
    /// 즉시 청구 가능 (window 열림 + 자격 있음).
    Claimable,
    /// 이미 청구됨.
    Claimed,
    /// claim window 가 만료됨.
    Expired,
}

/// 에어드랍 클레임 권리 — source / token / amount / proof / window / status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AirdropClaim {
    /// 에어드랍을 발행한 프로토콜.
    pub source: ProtocolRef,
    /// 청구 가능한 토큰.
    pub claimable: TokenRef,
    /// 청구 가능한 토큰 양 (base unit).
    #[tsify(type = "string")]
    pub amount: U256,
    /// 머클 클레임에 필요한 proof. 비-머클 방식은 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub proof: Option<MerkleProof>,
    /// 클레임 가능 기간 (start, end).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub claim_window: Option<(Time, Time)>,
    /// 클레임 lifecycle 상태.
    pub status: ClaimStatus,
}
