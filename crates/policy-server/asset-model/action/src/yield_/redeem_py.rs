//! `RedeemPyAction` — recombine the PT+YT pair back into a token/SY.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::YieldVenue;

/// Redeem the PT+YT pair to a token or SY (`redeemPyToToken` / `redeemPyToSy`).
///
/// Pre-expiry this requires equal PT and YT; post-expiry PT alone redeems.
/// `external_token` is the output token for `redeemPyToToken`; `None` for
/// `redeemPyToSy`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RedeemPyAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// YT contract identifying the PT+YT position.
    #[tsify(type = "string")]
    pub yt: Address,
    /// Plain external output token (`redeemPyToToken`); `None` for `redeemPyToSy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub external_token: Option<TokenRef>,
    /// PY (PT=YT) amount redeemed, U256.
    #[tsify(type = "string")]
    pub net_py_in: U256,
    /// Minimum output amount (token or SY) (slippage floor), U256.
    #[tsify(type = "string")]
    pub min_output: U256,
    /// Recipient of the output (`receiver` arg).
    #[tsify(type = "string")]
    pub recipient: Address,
}
