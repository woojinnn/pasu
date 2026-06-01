//! LP share shapes — Pooled (V2/Curve/Balancer) vs Concentrated (V3/V4/Joe LB)
//! vs Custom (escape hatch).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsify_next::Tsify;

use super::token_ref::TokenRef;
use crate::primitives::{Weight, U128, U256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
/// How an LP position's share is represented on-chain.
pub enum ShareForm {
    /// Fungible ERC20 LP tokens (e.g. Uniswap V2, Curve, Balancer pool shares).
    Fungible,
    /// Non-fungible position represented as an NFT (e.g. Uniswap V3/V4 positions).
    NonFungible,
}

/// The price-distribution shape of an LP share.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LpShape {
    /// Liquidity spread proportionally across the whole pool — Uniswap V2,
    /// Curve V1, Balancer.
    Pooled {
        /// Per-asset pool weights (e.g. Balancer weighted pools); `None` for
        /// uniformly weighted pools.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        weights: Option<Vec<Weight>>,
    },

    /// Liquidity concentrated within a price/tick range — Uniswap V3/V4,
    /// Trader Joe LB, Maverick.
    Concentrated {
        /// The range specification the liquidity is concentrated within.
        range: RangeSpec,
        /// Uncollected fees accrued to the position, as `(token, raw amount)`
        /// pairs.
        #[serde(default)]
        #[tsify(type = "Array<[TokenRef, string]>")]
        fees_owed: Vec<(TokenRef, U256)>,
    },

    /// An LP that fits neither shape above — escape hatch.
    Custom {
        /// Identifier of the originating protocol.
        protocol: String,
        /// Opaque protocol-specific payload preserved verbatim.
        #[tsify(type = "unknown")]
        raw: Value,
    },
}

/// The range over which concentrated liquidity is provided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RangeSpec {
    /// Tick-based concentration as used by Uniswap V3/V4.
    Tick {
        /// Lower tick boundary of the position (inclusive).
        lower: i32,
        /// Upper tick boundary of the position (inclusive).
        upper: i32,
        /// Liquidity amount held across the tick range.
        #[tsify(type = "string")]
        liquidity: U128,
    },

    /// Bin-based distribution as used by Trader Joe Liquidity Book.
    Bin {
        /// Identifier of the currently active (in-price) bin.
        active_id: u32,
        /// Liquidity distribution as `(bin id, liquidity)` pairs.
        #[tsify(type = "Array<[number, string]>")]
        distribution: Vec<(u32, U128)>,
    },

    /// A range format that differs from the above — e.g. Maverick.
    Custom {
        /// Identifier of the originating protocol.
        protocol: String,
        /// Opaque protocol-specific payload preserved verbatim.
        #[tsify(type = "unknown")]
        raw: Value,
    },
}
