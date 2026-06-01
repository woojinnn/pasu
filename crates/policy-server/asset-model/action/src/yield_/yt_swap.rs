//! `YtSwapAction` — swap a token/SY into YT, or YT back into a token/SY.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::{MarketTokensLiveInputs, YieldVenue};

/// Direction of a YT swap on a Pendle market (the four `swapExact*Yt*` entries).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum YtSwapDirection {
    /// `swapExactTokenForYt`: pay an external token, receive YT.
    TokenForYt,
    /// `swapExactSyForYt`: pay SY, receive YT.
    SyForYt,
    /// `swapExactYtForToken`: pay YT, receive an external token.
    YtForToken,
    /// `swapExactYtForSy`: pay YT, receive SY.
    YtForSy,
}

/// Swap involving YT on a Pendle market.
///
/// Models the `ActionSwapYTV3` facet — the YT mirror of [`PtSwapAction`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct YtSwapAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// Pendle market the swap routes through.
    #[tsify(type = "string")]
    pub market: Address,
    /// Which YT swap direction (token/SY ↔ YT).
    pub direction: YtSwapDirection,
    /// Plain external token side, present for `TokenForYt` / `YtForToken`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub external_token: Option<TokenRef>,
    /// Exact input amount (units depend on `direction`), U256.
    #[tsify(type = "string")]
    pub exact_amount_in: U256,
    /// Minimum acceptable output amount (slippage floor), U256.
    #[tsify(type = "string")]
    pub min_amount_out: U256,
    /// Recipient of the output (`receiver` arg).
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Market-derived instruments + maturity, fetched at simulation time.
    pub live_inputs: MarketTokensLiveInputs,
}
