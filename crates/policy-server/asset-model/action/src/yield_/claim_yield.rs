//! `ClaimYieldAction` — claim accrued interest + rewards (`redeemDueInterestAndRewards`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::Address;

use super::YieldVenue;

/// Claim due interest and rewards for the user across SYs / YTs / markets
/// (`redeemDueInterestAndRewards(user, sys[], yts[], markets[])`).
///
/// No fund leaves the user's control beyond claiming owed yield; the address
/// lists identify which SY/YT/market positions are swept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimYieldAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// User whose accrued yield is claimed (`user` arg).
    #[tsify(type = "string")]
    pub user: Address,
    /// SY contracts to sweep rewards from.
    #[tsify(type = "string[]")]
    pub sys: Vec<Address>,
    /// YT contracts to sweep interest from.
    #[tsify(type = "string[]")]
    pub yts: Vec<Address>,
    /// Market (LP) contracts to sweep rewards from.
    #[tsify(type = "string[]")]
    pub markets: Vec<Address>,
}
