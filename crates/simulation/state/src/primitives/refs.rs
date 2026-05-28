//! 가벼운 식별자 (Ref) 들. 본체는 외부 registry 나 다른 entity 에 있고,
//! 여기서는 join key 로만 사용.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::{address::Address, chain::ChainId, decimal::Weight};

/// 프로토콜 식별자. name 은 Defillama 컨벤션 (예: `aave_v3`, `uniswap_v3`, "hyperliquid").
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ProtocolRef {
    /// Defillama 컨벤션 프로토콜 이름.
    pub name: String,
    /// 프로토콜 메이저 / 마이너 버전 (예: "v3", "1.0"). 미지정 시 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub version: Option<String>,
    /// off-chain venue (Hyperliquid/dYdX 등) 는 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub chain: Option<ChainId>,
    /// Morpho Blue 처럼 sub-market 이 있는 경우.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub market: Option<String>,
}

impl ProtocolRef {
    /// name 만 지정한 `ProtocolRef` 생성. version / chain / market 은 `None`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            chain: None,
            market: None,
        }
    }

    /// `version` 을 채워 반환하는 builder.
    pub fn with_version(mut self, v: impl Into<String>) -> Self {
        self.version = Some(v.into());
        self
    }

    /// `chain` 을 채워 반환하는 builder.
    pub fn with_chain(mut self, c: ChainId) -> Self {
        self.chain = Some(c);
        self
    }
}

/// Pool 식별자. V2/V3 는 `pool_addr`, V4 는 `pool_id`, 오프체인은 `pool_id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PoolRef {
    /// Pool 이 속한 프로토콜 식별자.
    pub protocol: ProtocolRef,
    /// V2 / V3 등 EVM pool 의 컨트랙트 주소.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub pool_addr: Option<Address>,
    /// V4 `PoolId` bytes32 의 hex 또는 off-chain id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub pool_id: Option<String>,
    /// V3 의 feeTier 처럼 fee 가 pool key 의 일부인 경우. bps × 100 단위
    /// (예: 0.05% = 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub fee_tier: Option<Weight>,
}

/// 거래 venue (CEX-like, perp DEX 등).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct VenueRef {
    /// "hyperliquid", `gmx_v2`, `dydx_v4`, `uniswap_x` 등.
    pub name: String,
    /// EVM venue 의 settlement chain. 순수 off-chain venue 는 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub chain: Option<ChainId>,
}

impl VenueRef {
    /// name 만 지정한 `VenueRef` 생성. chain 은 `None`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            chain: None,
        }
    }
}

/// Market 식별자. 보통 "ETH-USD", "BTC-PERP" 같은 symbol.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct MarketRef {
    /// 거래 페어 또는 자산의 venue-내 심볼.
    pub symbol: String,
    /// 본 market 을 호스팅하는 venue.
    pub venue: VenueRef,
}
