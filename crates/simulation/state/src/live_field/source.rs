//! LiveField 의 출처 (DataSource) 정의. 어디서 어떻게 가져오는지의 메타.
//!
//! `value` 자체는 LiveField 안에 들고, source 는 갱신 주체가 보는 명세.

use serde::{Deserialize, Serialize};

use crate::primitives::{Address, ChainId};

/// 오라클 공급자.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleProvider {
    Pyth,
    Chainlink,
    RedStone,
    /// 그 외 공급자는 이름만 보존.
    Other(String),
}

/// 외부 API 호출 시 인증 방식.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    None,
    Bearer { token_env: String },
    HmacSig { key_env: String },
    Custom(String),
}

/// Sync orchestrator 가 사용하는 데이터 출처.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataSource {
    /// `eth_call` 같은 view 함수.
    OnchainView {
        chain: ChainId,
        contract: Address,
        function: String,
        /// 결과를 어떻게 decode 할지 식별자 (외부 registry).
        decoder_id: String,
    },

    /// 표준 오라클 피드.
    OracleFeed {
        provider: OracleProvider,
        feed_id: String,
    },

    /// REST/WebSocket venue API (Hyperliquid, GMX subgraph, dYdX indexer 등).
    VenueApi {
        endpoint: String,
        parser_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<AuthSpec>,
    },

    /// 다른 LiveField 들에서 계산. reducer 가 in-place 갱신.
    DerivedFrom {
        inputs: Vec<FieldRef>,
        calc_id: String,
    },

    /// 사용자가 직접 입력한 값 (e.g., manual override).
    UserSupplied,
}

/// 다른 LiveField 를 가리키는 참조 (DerivedFrom 의 inputs 에 사용).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum FieldRef {
    TokenField {
        /// TokenKey 를 JSON 으로 직렬화한 문자열. 순환 의존을 피하기 위해 문자열로
        /// 들고 다닌다. (LiveField 자체가 token 안에 박혀 있으므로 TokenKey 를
        /// 직접 import 하면 module cycle 위험.)
        token_key_json: String,
        field: TokenFieldName,
    },
    PositionField {
        position_id: String,
        field: PositionFieldName,
    },
    PendingField {
        pending_id: String,
        field: PendingFieldName,
    },
    /// gas_price, eth_usd 등 wallet/position 무관 전역 값.
    Global { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenFieldName {
    PriceUsd,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionFieldName {
    HealthFactor,
    Ltv,
    LiquidationThreshold,
    MarkPrice,
    LiqPrice,
    UnrealizedPnl,
    FundingOwed,
    Leverage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingFieldName {
    Status,
    /// 부분 fill 비율 등.
    FillRatio,
}
