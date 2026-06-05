//! `RemoveMarketLiquidityAction` — remove liquidity from a Pendle market (burn LP).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::{MarketTokensLiveInputs, YieldVenue};

/// Which `removeLiquidity*` entry on the `ActionAddRemoveLiqV3` facet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum RemoveLiquidityKind {
    /// `removeLiquidityDualTokenAndPt`: receive an external token + PT.
    DualTokenAndPt,
    /// `removeLiquidityDualSyAndPt`: receive SY + PT.
    DualSyAndPt,
    /// `removeLiquiditySinglePt`: receive only PT.
    SinglePt,
    /// `removeLiquiditySingleToken`: zap out to a single external token.
    SingleToken,
    /// `removeLiquiditySingleSy`: zap out to a single SY amount.
    SingleSy,
}

/// Remove liquidity from a Pendle market.
///
/// `net_lp_in` is the LP burned; the `min_*_out` fields are the slippage floors
/// for whichever outputs the chosen `kind` produces (others zero).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RemoveMarketLiquidityAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// Pendle market the liquidity is removed from.
    #[tsify(type = "string")]
    pub market: Address,
    /// Which remove-liquidity entry.
    pub kind: RemoveLiquidityKind,
    /// Plain external token received, present for the `*Token*` kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub external_token: Option<TokenRef>,
    /// LP tokens burned, U256.
    #[tsify(type = "string")]
    pub net_lp_in: U256,
    /// Minimum external-token out (0 if not a token kind), U256.
    #[tsify(type = "string")]
    pub min_token_out: U256,
    /// Minimum PT out (0 if PT not returned), U256.
    #[tsify(type = "string")]
    pub min_pt_out: U256,
    /// Minimum SY out (0 if SY not returned), U256.
    #[tsify(type = "string")]
    pub min_sy_out: U256,
    /// Recipient of the output (`receiver` arg).
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Market-derived instruments + maturity, fetched at simulation time.
    pub live_inputs: MarketTokensLiveInputs,
}
