//! `MintPyAction` — split a token/SY into the PT+YT pair.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::YieldVenue;

/// Mint the PT+YT pair from a token or SY (`mintPyFromToken` / `mintPyFromSy`).
///
/// `yt` is the YT contract (which, with its paired PT, defines the position).
/// `external_token` is the input token for `mintPyFromToken`; `None` for
/// `mintPyFromSy` (input is SY). `net_input` is the input amount; `min_py_out`
/// the minimum PT(=YT) minted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MintPyAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// YT contract identifying the PT+YT position.
    #[tsify(type = "string")]
    pub yt: Address,
    /// Plain external input token (`mintPyFromToken`); `None` for `mintPyFromSy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub external_token: Option<TokenRef>,
    /// Input amount (external token, or SY when `external_token` is `None`), U256.
    #[tsify(type = "string")]
    pub net_input: U256,
    /// Minimum PY (PT=YT) amount minted (slippage floor), U256.
    #[tsify(type = "string")]
    pub min_py_out: U256,
    /// Recipient of the minted PT+YT (`receiver` arg).
    #[tsify(type = "string")]
    pub recipient: Address,
}
