//! Lightweight identifier (`Ref`) types. The full entity lives in an external
//! registry or another entity; these are used only as join keys.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::{address::Address, chain::ChainId, decimal::Weight};

/// Protocol identifier. `name` follows the `DefiLlama` convention (e.g.
/// "`aave_v3`", "`uniswap_v3`", "hyperliquid").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ProtocolRef {
    /// Protocol name in `DefiLlama` convention (e.g. "`aave_v3`", "`uniswap_v3`").
    pub name: String,
    /// Optional protocol version (e.g. "v3"); `None` when not versioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub version: Option<String>,
    /// Chain the protocol is deployed on; `None` for off-chain venues
    /// (Hyperliquid, dYdX, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub chain: Option<ChainId>,
    /// Sub-market identifier, used when a protocol has distinct sub-markets
    /// (e.g. Morpho Blue).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub market: Option<String>,
}

impl ProtocolRef {
    /// Creates a `ProtocolRef` with the given name and all optional fields unset.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            chain: None,
            market: None,
        }
    }

    /// Returns this ref with the protocol version set (builder style).
    pub fn with_version(mut self, v: impl Into<String>) -> Self {
        self.version = Some(v.into());
        self
    }

    /// Returns this ref with the deployment chain set (builder style).
    #[must_use]
    pub fn with_chain(mut self, c: ChainId) -> Self {
        self.chain = Some(c);
        self
    }
}

/// Pool identifier. Uses `pool_addr` for V2/V3, `pool_id` for V4, and `pool_id`
/// for off-chain pools.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PoolRef {
    /// Protocol this pool belongs to.
    pub protocol: ProtocolRef,
    /// On-chain pool contract address (Uniswap V2/V3 style); `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub pool_addr: Option<Address>,
    /// Hex of the V4 `PoolId` (bytes32) or an off-chain pool id; `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub pool_id: Option<String>,
    /// Fee tier when fee is part of the pool key (e.g. Uniswap V3 `feeTier`),
    /// in units of bps x 100 (e.g. 0.05% = 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub fee_tier: Option<Weight>,
}

/// Trading venue identifier (CEX-like venues, perp DEXes, etc.).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct VenueRef {
    /// Venue name (e.g. "hyperliquid", "`gmx_v2`", "`dydx_v4`", "`uniswap_x`").
    pub name: String,
    /// Chain the venue is on; `None` for off-chain venues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub chain: Option<ChainId>,
}

impl VenueRef {
    /// Creates a `VenueRef` with the given name and no chain set.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            chain: None,
        }
    }
}

/// Market identifier, typically a symbol such as "ETH-USD" or "BTC-PERP".
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MarketRef {
    /// Market symbol (e.g. "ETH-USD", "BTC-PERP").
    pub symbol: String,
    /// Venue this market trades on.
    pub venue: VenueRef,
}
