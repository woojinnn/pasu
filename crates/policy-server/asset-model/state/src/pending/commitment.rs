//! `AssetCommitment` — how a single pending action ties up an asset.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, U256};
use crate::token::TokenRef;

/// How a pending action affects an asset; input to the spec §6 committed-update rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetCommitment {
    /// Cap-style — a venue/spender may pull up to `max_out` (`UniswapX`, permit).
    SpendCap {
        /// Token whose spend is capped.
        token: TokenRef,
        /// Maximum amount that may be spent (raw on-chain value).
        #[tsify(type = "string")]
        max_out: U256,
    },

    /// Hard-lock — already held by the venue/contract (e.g. perp margin lock); already reflected in the balance itself.
    HardLock {
        /// Token that is locked.
        token: TokenRef,
        /// Amount currently locked (raw on-chain value).
        #[tsify(type = "string")]
        locked: U256,
    },

    /// Permit issuance only — the token is not locked, but spend authority is granted.
    PermitCap {
        /// Token the permit applies to.
        token: TokenRef,
        /// Address granted spend authority over the token.
        #[tsify(type = "string")]
        spender: Address,
        /// Maximum amount the spender may pull (raw on-chain value).
        #[tsify(type = "string")]
        max_out: U256,
    },

    /// No commitment (reduce-only or receive-side orders).
    None,
}

impl AssetCommitment {
    /// Returns this commitment's contribution to the committed total for `key`; spec §6.1.
    /// - `SpendCap` / `PermitCap` → added to the committed total
    /// - `HardLock` → already reflected in the balance, not added
    /// - None → 0
    #[must_use]
    pub fn cap_for(&self, key: &crate::token::TokenKey) -> U256 {
        match self {
            Self::SpendCap { token, max_out } if &token.key == key => *max_out,
            Self::PermitCap { token, max_out, .. } if &token.key == key => *max_out,
            _ => U256::ZERO,
        }
    }
}
