//! NonceKey — pending 간 충돌 검사용 식별자.

use serde::{Deserialize, Serialize};

use crate::primitives::{Address, U256};

/// 32-byte hash (TxHash, OrderHash). hex string "0x..".
pub type B256 = String;

/// TxHash.
pub type TxHash = B256;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NonceKey {
    /// Permit2 의 (word, bit) 비트맵 nonce.
    Permit2 { word: U256, bit: u8 },
    /// EIP-2612 의 (token, nonce).
    Eip2612 { token: Address, nonce: U256 },
    /// off-chain order hash.
    OrderHash { hash: B256 },
}
