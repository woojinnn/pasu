//! `AddMarketLiquidityAction` — add liquidity to a Pendle market (mint LP).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::{MarketTokensLiveInputs, YieldVenue};

/// Which `addLiquidity*` entry on the `ActionAddRemoveLiqV3` facet.
///
/// `*KeepYt` variants return the residual YT to the user instead of zapping it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum AddLiquidityKind {
    /// `addLiquidityDualTokenAndPt`: supply an external token + PT.
    DualTokenAndPt,
    /// `addLiquidityDualSyAndPt`: supply SY + PT.
    DualSyAndPt,
    /// `addLiquiditySinglePt`: supply only PT.
    SinglePt,
    /// `addLiquiditySingleToken`: zap in from a single external token.
    SingleToken,
    /// `addLiquiditySingleSy`: zap in from a single SY amount.
    SingleSy,
    /// `addLiquiditySingleTokenKeepYt`: single-token, keep residual YT.
    SingleTokenKeepYt,
    /// `addLiquiditySingleSyKeepYt`: single-SY, keep residual YT.
    SingleSyKeepYt,
}

/// Add liquidity to a Pendle market.
///
/// The amount fields capture whichever inputs the chosen `kind` supplies (the
/// others are zero): `net_token_in` (external token / `TokenInput.netTokenIn`),
/// `net_pt_in` (`netPtDesired`), `net_sy_in` (`netSyDesired`/`netSyIn`).
/// `min_lp_out` is the LP slippage floor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AddMarketLiquidityAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// Pendle market receiving the liquidity.
    #[tsify(type = "string")]
    pub market: Address,
    /// Which add-liquidity entry.
    pub kind: AddLiquidityKind,
    /// Plain external token supplied, present for the `*Token*` kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub external_token: Option<TokenRef>,
    /// External-token amount supplied (0 if not a token kind), U256.
    #[tsify(type = "string")]
    pub net_token_in: U256,
    /// PT amount supplied (0 if not supplied), U256.
    #[tsify(type = "string")]
    pub net_pt_in: U256,
    /// SY amount supplied (0 if not supplied), U256.
    #[tsify(type = "string")]
    pub net_sy_in: U256,
    /// Minimum LP tokens minted (slippage floor), U256.
    #[tsify(type = "string")]
    pub min_lp_out: U256,
    /// Recipient of the LP (`receiver` arg).
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Market-derived instruments + maturity, fetched at simulation time.
    pub live_inputs: MarketTokensLiveInputs,
}
