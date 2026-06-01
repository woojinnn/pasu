//! LP share shapes: pooled, concentrated, or custom.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsify_next::Tsify;

use super::token_ref::TokenRef;
use crate::primitives::{Weight, U128, U256};

/// Whether an LP share is fungible or non-fungible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum ShareForm {
    /// ERC-20 LP share, such as Uniswap V2 or Curve LP tokens.
    Fungible,
    /// ERC-721 LP share, such as Uniswap V3/V4 position NFTs.
    NonFungible,
}

/// Price distribution shape for an LP share.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LpShape {
    /// Pro-rata share of the whole pool.
    Pooled {
        /// Per-asset pool weights; `None` for equal weights.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        weights: Option<Vec<Weight>>,
    },

    /// Liquidity concentrated in a price, tick, or bin range.
    Concentrated {
        /// Concentrated range held by this share.
        range: RangeSpec,
        /// Uncollected fees accrued by this share as `(token, base units)`.
        #[serde(default)]
        #[tsify(type = "Array<[TokenRef, string]>")]
        fees_owed: Vec<(TokenRef, U256)>,
    },

    /// Escape hatch for LP shapes that do not fit the standard models.
    Custom {
        /// Protocol identifier for display.
        protocol: String,
        /// Protocol-specific raw JSON.
        #[tsify(type = "unknown")]
        raw: Value,
    },
}

/// Range representation for `LpShape::Concentrated`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RangeSpec {
    /// Tick range, used by Uniswap V3/V4 style positions.
    Tick {
        /// Inclusive lower tick.
        lower: i32,
        /// Inclusive upper tick.
        upper: i32,
        /// Liquidity inside the range.
        #[tsify(type = "string")]
        liquidity: U128,
    },

    /// Bin distribution, used by Trader Joe LB style positions.
    Bin {
        /// Current active bin id.
        active_id: u32,
        /// Liquidity distribution held by this share per bin id.
        #[tsify(type = "Array<[number, string]>")]
        distribution: Vec<(u32, U128)>,
    },

    /// Custom range for protocols with different shapes.
    Custom {
        /// Protocol identifier for display.
        protocol: String,
        /// Protocol-specific raw JSON.
        #[tsify(type = "unknown")]
        raw: Value,
    },
}
