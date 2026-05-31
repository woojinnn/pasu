//! `AirdropClaim` — airdrop claim entitlement.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{ProtocolRef, Time, U256};
use crate::token::TokenRef;

/// Merkle claim proof. Encoded as an array of hex strings to preserve depth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MerkleProof {
    /// Index of this leaf within the Merkle tree.
    pub leaf_index: u64,
    /// Sibling hashes along the path to the root, as hex strings ("0x..").
    pub siblings: Vec<String>,
}

/// Lifecycle status of an airdrop claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    /// Address qualifies for the airdrop but the claim is not yet open.
    Eligible,
    /// Claim is open and can be claimed now.
    Claimable,
    /// Tokens have already been claimed.
    Claimed,
    /// Claim window has passed; the airdrop can no longer be claimed.
    Expired,
}

/// An airdrop claim entitlement held by a position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AirdropClaim {
    /// Protocol distributing the airdrop.
    pub source: ProtocolRef,
    /// Token that can be claimed.
    pub claimable: TokenRef,
    /// Claimable amount as a raw on-chain integer (U256).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Optional Merkle proof required to claim, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub proof: Option<MerkleProof>,
    /// Optional claim window as a (start, end) time pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub claim_window: Option<(Time, Time)>,
    /// Current lifecycle status of the claim.
    pub status: ClaimStatus,
}
