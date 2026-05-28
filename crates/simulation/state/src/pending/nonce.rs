//! `NonceKey` — pending 간 충돌 검사용 식별자.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, U256};

/// 32-byte hash (`TxHash`, `OrderHash`). hex string "0x..".
pub type B256 = String;

/// 32-byte transaction hash alias.
pub type TxHash = B256;

/// pending 간 충돌 검사용 nonce / hash 식별자.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NonceKey {
    /// Permit2 의 (word, bit) 비트맵 nonce.
    Permit2 {
        /// nonce 비트맵의 word index.
        #[tsify(type = "string")]
        word: U256,
        /// word 내 bit index (0-255).
        bit: u8,
    },
    /// EIP-2612 의 (token, nonce).
    Eip2612 {
        /// 본 nonce 가 속한 token contract.
        #[tsify(type = "string")]
        token: Address,
        /// owner-level monotonic nonce.
        #[tsify(type = "string")]
        nonce: U256,
    },
    /// off-chain order hash.
    OrderHash {
        /// 32-byte order hash (hex string).
        hash: B256,
    },
}
