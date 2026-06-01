//! `NonceKey` — identifier used to detect nonce collisions between pending actions.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, U256};

/// 32-byte hash (`TxHash`, `OrderHash`). hex string "0x..".
pub type B256 = String;

/// `TxHash`.
pub type TxHash = B256;

/// Unique key identifying a nonce across the different nonce schemes a pending action can use.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NonceKey {
    /// Permit2 unordered (word, bit) bitmap nonce.
    Permit2 {
        /// Bitmap word index that holds the nonce bit.
        #[tsify(type = "string")]
        word: U256,
        /// Bit position within the bitmap word.
        bit: u8,
    },
    /// EIP-2612 sequential (token, nonce) pair.
    Eip2612 {
        /// ERC-20 token contract whose permit nonce is tracked.
        #[tsify(type = "string")]
        token: Address,
        /// Sequential permit nonce value for the token.
        #[tsify(type = "string")]
        nonce: U256,
    },
    /// Off-chain order hash used as a one-shot nonce.
    OrderHash {
        /// Hash of the off-chain order (hex "0x..").
        hash: B256,
    },
}
