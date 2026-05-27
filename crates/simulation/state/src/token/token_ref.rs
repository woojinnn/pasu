//! TokenRef — TokenKind 안에서 다른 토큰을 가리키는 가벼운 식별자.
//!
//! 본체 holding 은 외부 `tokens` map 에 있으므로, kind 안에서는 key 만 들고 다닌다.
//! (예: aUSDC.kind = YieldReceipt { underlying: TokenRef(USDC.key) })

use serde::{Deserialize, Serialize};

use super::key::TokenKey;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenRef {
    pub key: TokenKey,
}

impl TokenRef {
    pub fn new(key: TokenKey) -> Self {
        Self { key }
    }
}

impl From<TokenKey> for TokenRef {
    fn from(key: TokenKey) -> Self {
        Self { key }
    }
}
