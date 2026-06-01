//! `PtSwapAction` — swap a token/SY into PT, or PT back into a token/SY.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::{MarketTokensLiveInputs, YieldVenue};

/// Direction of a PT swap on a Pendle market (the four `swapExact*Pt*` entries).
///
/// The PT/SY instruments are derived from `market`; only the external token side
/// (when present) appears in calldata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PtSwapDirection {
    /// `swapExactTokenForPt`: pay an external token, receive PT.
    TokenForPt,
    /// `swapExactSyForPt`: pay SY, receive PT.
    SyForPt,
    /// `swapExactPtForToken`: pay PT, receive an external token.
    PtForToken,
    /// `swapExactPtForSy`: pay PT, receive SY.
    PtForSy,
}

/// Swap involving PT on a Pendle market.
///
/// Models the `ActionSwapPTV3` facet. `exact_amount_in` is the exact input
/// amount (token / SY / PT depending on `direction`); `min_amount_out` is the
/// slippage floor. `external_token` is the plain token side when the swap zaps
/// in/out of a non-SY token (`TokenForPt` / `PtForToken`); for the SY-side
/// directions it is `None` (SY is `market`-derived, surfaced via enrichment).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PtSwapAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// Pendle market the swap routes through.
    #[tsify(type = "string")]
    pub market: Address,
    /// Which PT swap direction (token/SY ↔ PT).
    pub direction: PtSwapDirection,
    /// Plain external token side, present for `TokenForPt` / `PtForToken`.
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
