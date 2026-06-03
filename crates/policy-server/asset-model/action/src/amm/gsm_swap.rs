//! `GsmSwap` action — Aave GHO Stability Module (GSM) fixed-price swap.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::AmmVenue;

/// A GHO Stability Module buy/sell — a fixed-price swap between GHO and the
/// GSM's single non-GHO asset (USDC / USDT). Distinct from [`super::SwapAction`]
/// because a GSM is not an AMM pool: there is no route, no `PoolState`, and no
/// slippage parameter — the price is set by the GSM's fee/price strategy. The
/// calldata carries only a single user bound (`minAmount` on buy / `maxAmount`
/// on sell) plus a receiver, so this is a faithful static decode of that intent
/// without pretending it is an ordinary exact-in / exact-out AMM swap.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GsmSwapAction {
    /// GSM venue (`AmmVenue::AaveGsm { chain, gsm }`).
    pub venue: AmmVenue,
    /// The GSM's non-GHO asset (USDC / USDT) — bought on `buy`, sold on `sell`.
    pub asset: TokenRef,
    /// GHO token (the other side of every GSM swap).
    pub gho: TokenRef,
    /// Swap direction (buy = GHO→asset, sell = asset→GHO).
    pub side: GsmSwapSide,
    /// The user-signed bound from calldata: minimum `asset` out on `buy`,
    /// maximum `asset` in on `sell` (`minAmount` / `maxAmount`), U256 hex.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Recipient of the bought asset (`buy`) or the received GHO (`sell`).
    #[tsify(type = "string")]
    pub recipient: Address,
}

/// Direction of a GSM swap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum GsmSwapSide {
    /// Buy the GSM asset with GHO (`buyAsset`); `amount` = minimum asset out.
    BuyAsset,
    /// Sell the GSM asset for GHO (`sellAsset`); `amount` = maximum asset in.
    SellAsset,
}

impl GsmSwapSide {
    /// Stable `snake_case` string used in the Cedar context (`"buy_asset"` /
    /// `"sell_asset"`); matches the `#[serde(rename_all = "snake_case")]` tag.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BuyAsset => "buy_asset",
            Self::SellAsset => "sell_asset",
        }
    }
}
