//! `Bridge::Send` action — outbound cross-chain bridge of a token.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ChainId, U256};
use policy_state::token::TokenRef;

use super::BridgeVenue;

/// Outbound bridge: the user escrows/burns `src_token` on the source chain to
/// receive `dst_token` at `dst_recipient` on `dst_chain_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct BridgeSendAction {
    /// Bridge entry contract the user calls.
    pub venue: BridgeVenue,
    /// Token escrowed/burned on the source chain.
    pub src_token: TokenRef,
    /// Amount of `src_token` sent.
    #[tsify(type = "string")]
    pub input_amount: U256,
    /// Destination chain (`CAIP-2`, e.g. `eip155:10`).
    pub dst_chain_id: ChainId,
    /// Recipient on the destination chain — the highest-value bridge phishing field.
    pub dst_recipient: BridgeRecipient,
    /// Token delivered on the destination chain. `None` when not statically
    /// known from calldata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub dst_token: Option<TokenRef>,
    /// Amount delivered on the destination (input minus relayer/LP fee).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub output_amount: Option<U256>,
    /// Relayer granted temporary exclusive fill rights (zero / absent = open).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub exclusive_relayer: Option<Address>,
    /// Whether a destination-execution message (compose / arbitrary call) is attached.
    pub has_message: bool,
}

/// Destination recipient — an EVM 20-byte address, or a raw 32-byte word for
/// cross-VM destinations (e.g. Solana).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BridgeRecipient {
    /// 20-byte EVM recipient address.
    Evm {
        /// Recipient address.
        #[tsify(type = "string")]
        address: Address,
    },
    /// Raw 32-byte recipient (non-EVM destination), hex-encoded.
    Raw {
        /// 32-byte recipient word.
        bytes32: String,
    },
}
