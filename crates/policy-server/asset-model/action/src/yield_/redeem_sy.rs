//! `RedeemSyAction` — unwrap an SY back into a token (`redeemSyToToken`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::YieldVenue;

/// Unwrap SY shares back into an external token (`redeemSyToToken`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RedeemSyAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// SY contract redeemed from.
    #[tsify(type = "string")]
    pub sy: Address,
    /// External token received.
    pub external_token: TokenRef,
    /// SY shares redeemed, U256.
    #[tsify(type = "string")]
    pub net_sy_in: U256,
    /// Minimum external-token out (slippage floor), U256.
    #[tsify(type = "string")]
    pub min_token_out: U256,
    /// Recipient of the output token (`receiver` arg).
    #[tsify(type = "string")]
    pub recipient: Address,
}
