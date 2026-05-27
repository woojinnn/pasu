//! 가벼운 식별자 (Ref) 들. 본체는 외부 registry 나 다른 entity 에 있고,
//! 여기서는 join key 로만 사용.

use serde::{Deserialize, Serialize};

use super::{address::Address, chain::ChainId, decimal::Weight};

/// 프로토콜 식별자. name 은 Defillama 컨벤션 (예: "aave_v3", "uniswap_v3", "hyperliquid").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProtocolRef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// off-chain venue (Hyperliquid/dYdX 등) 는 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainId>,
    /// Morpho Blue 처럼 sub-market 이 있는 경우.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}

impl ProtocolRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            chain: None,
            market: None,
        }
    }

    pub fn with_version(mut self, v: impl Into<String>) -> Self {
        self.version = Some(v.into());
        self
    }

    pub fn with_chain(mut self, c: ChainId) -> Self {
        self.chain = Some(c);
        self
    }
}

/// Pool 식별자. V2/V3 는 pool_addr, V4 는 pool_id, 오프체인은 pool_id.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PoolRef {
    pub protocol: ProtocolRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_addr: Option<Address>,
    /// V4 PoolId bytes32 의 hex 또는 off-chain id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_id: Option<String>,
    /// V3 의 feeTier 처럼 fee 가 pool key 의 일부인 경우. bps × 100 단위
    /// (예: 0.05% = 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_tier: Option<Weight>,
}

/// 거래 venue (CEX-like, perp DEX 등).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VenueRef {
    /// "hyperliquid", "gmx_v2", "dydx_v4", "uniswap_x" 등.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<ChainId>,
}

impl VenueRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            chain: None,
        }
    }
}

/// Market 식별자. 보통 "ETH-USD", "BTC-PERP" 같은 symbol.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketRef {
    pub symbol: String,
    pub venue: VenueRef,
}
