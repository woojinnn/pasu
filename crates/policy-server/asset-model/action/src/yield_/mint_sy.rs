//! `MintSyAction` — wrap a token into its SY (`mintSyFromToken`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::YieldVenue;

/// Wrap an external token into SY shares (`mintSyFromToken`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MintSyAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// SY contract minted into.
    #[tsify(type = "string")]
    pub sy: Address,
    /// External token deposited.
    pub external_token: TokenRef,
    /// External-token amount deposited, U256.
    #[tsify(type = "string")]
    pub net_token_in: U256,
    /// Minimum SY shares minted (slippage floor), U256.
    #[tsify(type = "string")]
    pub min_sy_out: U256,
    /// Recipient of the SY (`receiver` arg).
    #[tsify(type = "string")]
    pub recipient: Address,
}
