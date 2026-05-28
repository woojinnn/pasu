//! TokenRef — TokenKind 안에서 다른 토큰을 가리키는 가벼운 식별자.
//!
//! 본체 holding 은 외부 `tokens` map 에 있으므로, kind 안에서는 key 만 들고 다닌다.
//! (예: aUSDC.kind = YieldReceipt { underlying: TokenRef(USDC.key) })

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::key::TokenKey;

/// `TokenKind` 안에서 다른 토큰을 가리키는 가벼운 ref.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TokenRef {
    /// 가리키는 토큰의 fungibility 단위 key.
    pub key: TokenKey,
}

impl TokenRef {
    /// `TokenKey` 로부터 `TokenRef` 생성.
    pub fn new(key: TokenKey) -> Self {
        Self { key }
    }
}

impl From<TokenKey> for TokenRef {
    fn from(key: TokenKey) -> Self {
        Self { key }
    }
}
