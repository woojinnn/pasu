//! `SignOrderAction` — a maker signs an EIP-712 marketplace order
//! (Seaport `OrderComponents`): an off-chain listing / offer.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Time, U256};

use super::item::MarketItem;
use super::venue::MarketplaceVenue;
use crate::Bytes;

/// Off-chain marketplace order signature: the maker commits to give `offer[]`
/// in exchange for `consideration[]`. The key drainer surface — a malicious
/// order can sign away an NFT for ~0, or (with `*_criteria` offer items) ANY
/// NFT in a collection. No on-chain tx at sign time; a taker fulfills later.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SignOrderAction {
    /// Settlement venue (Seaport).
    pub venue: MarketplaceVenue,
    /// Order maker (the signer).
    #[tsify(type = "string")]
    pub offerer: Address,
    /// Optional restricted-order zone (validator). Absent for open orders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub zone: Option<Address>,
    /// Items the maker GIVES.
    pub offer: Vec<MarketItem>,
    /// Items the maker RECEIVES (proceeds + fees + royalties, each to a recipient).
    pub consideration: Vec<MarketItem>,
    /// `full_open` | `partial_open` | `full_restricted` | `partial_restricted` | `contract`.
    pub order_type: String,
    /// Order validity start (unix seconds).
    pub start_time: Time,
    /// Order validity end / expiry (unix seconds).
    pub end_time: Time,
    /// Conduit key (bytes32 hex). `0x0…0` = direct Seaport conduit; non-zero
    /// routes transfers through a separately-approved Conduit operator.
    pub conduit_key: Bytes,
    /// Offerer's on-chain order nonce (Seaport `counter`).
    #[tsify(type = "string")]
    pub counter: U256,
}
