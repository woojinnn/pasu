//! LiveField 의 출처 (DataSource) 정의. 어디서 어떻게 가져오는지의 메타.
//!
//! `value` 자체는 LiveField 안에 들고, source 는 갱신 주체가 보는 명세.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{Address, ChainId};

/// 오라클 공급자.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum OracleProvider {
    /// Pyth Network.
    Pyth,
    /// Chainlink.
    Chainlink,
    /// `RedStone`.
    RedStone,
    /// 그 외 공급자는 이름만 보존.
    Other(String),
}

/// 외부 API 호출 시 인증 방식.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    /// 인증 불필요.
    None,
    /// Authorization header 의 bearer token. 값은 env var 에서.
    Bearer {
        /// token 을 담은 환경 변수 이름.
        token_env: String,
    },
    /// 요청에 HMAC 서명을 첨부. key 는 env var 에서.
    HmacSig {
        /// 서명 key 를 담은 환경 변수 이름.
        key_env: String,
    },
    /// venue 고유 인증 (이름만 보존).
    Custom(String),
}

/// Sync orchestrator 가 사용하는 데이터 출처.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataSource {
    /// `eth_call` 같은 view 함수.
    OnchainView {
        /// view 가 실행될 chain.
        chain: ChainId,
        /// 호출 대상 컨트랙트 주소.
        #[tsify(type = "string")]
        contract: Address,
        /// 호출 함수 (selector + 시그니처).
        function: String,
        /// 결과를 어떻게 decode 할지 식별자 (외부 registry).
        decoder_id: String,
    },

    /// 표준 오라클 피드.
    OracleFeed {
        /// 피드를 제공하는 oracle.
        provider: OracleProvider,
        /// provider-내 feed 식별자.
        feed_id: String,
    },

    /// REST/WebSocket venue API (Hyperliquid, GMX subgraph, dYdX indexer 등).
    VenueApi {
        /// 호출 endpoint URL.
        endpoint: String,
        /// 응답을 어떻게 parse 할지 식별자.
        parser_id: String,
        /// 호출 시 적용할 인증 명세. None = 공개 endpoint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        auth: Option<AuthSpec>,
    },

    /// 다른 `LiveField` 들에서 계산. reducer 가 in-place 갱신.
    DerivedFrom {
        /// 입력으로 사용할 다른 `LiveField` 들.
        inputs: Vec<FieldRef>,
        /// 어떤 계산식을 적용할지 식별자.
        calc_id: String,
    },

    /// 사용자가 직접 입력한 값 (e.g., manual override).
    UserSupplied,
}

/// 다른 `LiveField` 를 가리키는 참조 (`DerivedFrom` 의 `inputs` 에 사용).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum FieldRef {
    /// 한 token holding 의 `LiveField` 참조.
    TokenField {
        /// `TokenKey` 를 JSON 으로 직렬화한 문자열. 순환 의존을 피하기 위해 문자열로
        /// 들고 다닌다. (`LiveField` 자체가 token 안에 박혀 있으므로 `TokenKey` 를
        /// 직접 import 하면 module cycle 위험.)
        token_key_json: String,
        /// 참조 대상 필드.
        field: TokenFieldName,
    },
    /// 한 position 의 `LiveField` 참조.
    PositionField {
        /// 대상 position 의 id.
        position_id: String,
        /// 참조 대상 필드.
        field: PositionFieldName,
    },
    /// 한 pending 의 `LiveField` 참조.
    PendingField {
        /// 대상 pending 의 id.
        pending_id: String,
        /// 참조 대상 필드.
        field: PendingFieldName,
    },
    /// `gas_price`, `eth_usd` 등 wallet/position 무관 전역 값.
    Global {
        /// 전역 변수 이름.
        name: String,
    },
}

/// 한 token holding 의 `LiveField` 필드 이름.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum TokenFieldName {
    /// USD 가격.
    PriceUsd,
}

/// 한 position 의 `LiveField` 필드 이름.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PositionFieldName {
    /// Lending account 의 health factor.
    HealthFactor,
    /// Lending account 의 LTV.
    Ltv,
    /// Lending account 의 청산 임계값.
    LiquidationThreshold,
    /// Perp 의 mark price.
    MarkPrice,
    /// Perp 의 청산 price.
    LiqPrice,
    /// Perp 의 미실현 손익.
    UnrealizedPnl,
    /// Perp 의 미수령 / 미납 funding.
    FundingOwed,
    /// Perp 의 실효 leverage.
    Leverage,
}

/// 한 pending 의 `LiveField` 필드 이름.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PendingFieldName {
    /// pending 의 lifecycle 상태.
    Status,
    /// 부분 fill 비율 등.
    FillRatio,
}
