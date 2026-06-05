//! Lightweight identifier for another token referenced by a `TokenKind`.
//!
//! The actual holding lives in the external `tokens` map, so a kind only carries
//! the key (e.g. `aUSDC.kind = YieldReceipt { underlying: TokenRef(USDC.key) }`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::key::TokenKey;

/// Lightweight reference to another token, holding only its [`TokenKey`] while the
/// actual holding stays in the external `tokens` map.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TokenRef {
    /// Key of the referenced token in the `tokens` map.
    pub key: TokenKey,
}

impl TokenRef {
    /// Creates a `TokenRef` referencing the token identified by `key`.
    #[must_use]
    pub const fn new(key: TokenKey) -> Self {
        Self { key }
    }
}

impl From<TokenKey> for TokenRef {
    fn from(key: TokenKey) -> Self {
        Self { key }
    }
}
