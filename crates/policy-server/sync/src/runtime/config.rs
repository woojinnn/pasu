//! Sync 전체 설정 — RPC providers + Oracle catalogs + Venue endpoints.
//!
//! 단일 TOML (`scopeball-sync.toml`) 안에 세 섹션:
//! ```toml
//! [rpc.failover]
//! strategy = "priority"
//!
//! [rpc.chains."eip155:1"]
//! multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"
//!
//! [[rpc.chains."eip155:1".providers]]
//! name = "publicnode"
//! kind = "public"
//! url  = "https://ethereum-rpc.publicnode.com"
//! priority = 1
//!
//! [oracles.chainlink.chains."eip155:1".feeds]
//! "USDC/USD" = { address = "0x8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6", decimals = 8 }
//!
//! [oracles.pyth]
//! endpoint = "https://hermes.pyth.network"
//!
//! [oracles.pyth.feeds]
//! "ETH/USD" = { price_id = "0xff61491a..." }
//!
//! [venues.hyperliquid]
//! endpoint = "https://api.hyperliquid.xyz"
//! ```
//!
//! `${VAR}` 패턴은 환경변수로 치환. 변수가 없으면 빈 문자열로 들어가니
//! 시크릿은 `.env` 와 함께 git 제외.

use std::collections::BTreeMap;
use std::path::Path;

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

use simulation_state::ChainId;

use crate::error::SyncError;
use crate::fetchers::rpc::RpcConfig;

/// Sync crate 전체 설정.
///
/// `scopeball-sync.toml` 의 최상위. `RpcConfig` 가 chain/provider 를,
/// `OraclesConfig` 가 가격/메트릭 source 를, `VenuesConfig` 가 venue REST
/// 엔드포인트를 들고 있다.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// RPC providers + failover.
    #[serde(default)]
    pub rpc: RpcConfig,

    /// 오라클 카탈로그 (chainlink / pyth / ...).
    #[serde(default)]
    pub oracles: OraclesConfig,

    /// Venue API 엔드포인트.
    #[serde(default)]
    pub venues: VenuesConfig,
}

impl SyncConfig {
    /// TOML 파일에서 로드. `${VAR}` 패턴은 환경변수로 치환.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self, SyncError> {
        let text = std::fs::read_to_string(&path).map_err(|e| SyncError::FetchFailed {
            source_id: "config_file".into(),
            reason: format!("{}: {}", path.as_ref().display(), e),
        })?;
        Self::load_str(&text)
    }

    pub fn load_str(text: &str) -> Result<Self, SyncError> {
        let expanded = expand_env_vars(text);
        toml::from_str(&expanded).map_err(|e| SyncError::FetchFailed {
            source_id: "config_toml".into(),
            reason: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Oracles
// ---------------------------------------------------------------------------

/// 오라클 카탈로그.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OraclesConfig {
    /// On-chain Chainlink `AggregatorV3` feed 목록 (chain → feeds).
    #[serde(default)]
    pub chainlink: ChainlinkConfig,

    /// Pyth Hermes REST API + feed catalog. 사용 안 하면 생략.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pyth: Option<PythConfig>,

    /// Generic REST JSON oracles — `CoinGecko` / `CoinMarketCap` / Pyth Hermes 등.
    ///
    /// 키는 provider 의 canonical name (소문자 `snake_case`). 같은 이름이
    /// `DataSource::OracleFeed.provider` 의 `OracleProvider::Other(name)` 와
    /// 매칭돼 dispatch.
    #[serde(default)]
    pub rest: BTreeMap<String, RestOracleConfig>,
}

/// Chainlink feed catalog — chain 별로 (`feed_id` → contract address).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChainlinkConfig {
    /// chain id (CAIP-2) → 해당 체인의 feed 들.
    #[serde(default)]
    pub chains: BTreeMap<ChainId, ChainlinkChainConfig>,
}

/// 한 체인의 Chainlink feed 목록.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChainlinkChainConfig {
    /// `feed_id` (`"USDC/USD"` 등) → feed metadata.
    #[serde(default)]
    pub feeds: BTreeMap<String, ChainlinkFeedConfig>,
}

/// 한 Chainlink feed 의 contract metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainlinkFeedConfig {
    /// `AggregatorV3` contract 주소.
    pub address: Address,
    /// `decimals()` 반환값. 대부분 8 — 생략 시 default 8.
    #[serde(default = "default_chainlink_decimals")]
    pub decimals: u8,
}

const fn default_chainlink_decimals() -> u8 {
    8
}

/// Pyth Hermes (<https://hermes.pyth.network>) REST API + price feed id 카탈로그.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PythConfig {
    /// Hermes base URL.
    pub endpoint: String,
    /// `feed_id` (`"ETH/USD"` 등) → Pyth price feed id (32-byte hex).
    #[serde(default)]
    pub feeds: BTreeMap<String, PythFeedConfig>,
}

/// 한 Pyth feed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PythFeedConfig {
    /// Pyth price feed id, "0x" + 64 hex.
    pub price_id: String,
}

// ---------------------------------------------------------------------------
// Generic REST JSON oracle
// ---------------------------------------------------------------------------

/// 한 REST JSON oracle provider (`CoinGecko` / `CoinMarketCap` / Pyth Hermes ...)
/// 의 endpoint + 인증 + feed catalog.
///
/// Generic `RestJsonOracleFetcher` 가 이 설정만 받으면 `fetch_price` 가능 —
/// 새 provider 추가는 본 섹션 한 블록만 늘리면 된다 (Rust 코드 변경 0).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestOracleConfig {
    /// base URL — feed path 와 합쳐서 최종 endpoint 가 됨.
    pub base_url: String,

    /// HTTP header 기반 인증. 없으면 익명 호출.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<RestAuthConfig>,

    /// HTTP 타임아웃 (초). 기본 10.
    #[serde(default = "default_rest_timeout_sec")]
    pub timeout_sec: u64,

    /// `feed_id` (`"USDC/USD"` 등) → path + JSON pointer.
    #[serde(default)]
    pub feeds: BTreeMap<String, RestFeedConfig>,
}

const fn default_rest_timeout_sec() -> u64 {
    10
}

/// REST 인증 — header 한 개 (예: `X-CG-Pro-Api-Key`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestAuthConfig {
    /// HTTP header 이름.
    pub header_name: String,
    /// 값을 읽어올 환경변수 이름. 변수가 비어있으면 인증 헤더 미부착.
    pub env_var: String,
}

/// 한 REST feed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestFeedConfig {
    /// `base_url` 뒤에 붙는 경로 + 쿼리스트링.
    ///
    /// 예: `"/simple/price?ids=usd-coin&vs_currencies=usd"`
    pub path: String,

    /// RFC 6901 JSON pointer — 응답 body 에서 가격 숫자를 꺼낼 경로.
    ///
    /// 예: `"/usd-coin/usd"` →  `{"usd-coin": {"usd": 0.9998}}` 의 `0.9998`
    pub json_pointer: String,
}

// ---------------------------------------------------------------------------
// Venues
// ---------------------------------------------------------------------------

/// Venue API 엔드포인트 모음.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VenuesConfig {
    /// Hyperliquid REST API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperliquid: Option<HyperliquidConfig>,
}

/// Hyperliquid 설정.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HyperliquidConfig {
    /// API base URL — 보통 `https://api.hyperliquid.xyz`.
    pub endpoint: String,
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// `${VAR_NAME}` → `std::env::var("VAR_NAME")`. 변수가 없으면 빈 문자열.
///
/// `rpc/config.rs` 의 `RpcConfig::load_str` 에서도 동일한 치환이 필요해
/// crate-internal helper 로 공유한다.
pub(crate) fn expand_env_vars(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
                name.push(c2);
            }
            let val = std::env::var(&name).unwrap_or_default();
            out.push_str(&val);
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parses_full_config() {
        let toml_text = r#"
[rpc.chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"

[[rpc.chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
ws = false

[oracles.chainlink.chains."eip155:1".feeds]
"USDC/USD" = { address = "0x8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6", decimals = 8 }
"ETH/USD"  = { address = "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419" }

[oracles.pyth]
endpoint = "https://hermes.pyth.network"

[oracles.pyth.feeds]
"ETH/USD" = { price_id = "0xff61491a93114263cabb1d5ca0e6d6e5d8e8a4f0e6c8a4f0e6c8a4f0e6c8a4f0" }

[venues.hyperliquid]
endpoint = "https://api.hyperliquid.xyz"
"#;
        let cfg = SyncConfig::load_str(toml_text).unwrap();

        // RPC
        let chain = cfg.rpc.chain(&ChainId::ethereum_mainnet()).unwrap();
        assert_eq!(chain.providers.len(), 1);
        assert_eq!(chain.providers[0].name, "publicnode");

        // Chainlink
        let mainnet = cfg
            .oracles
            .chainlink
            .chains
            .get(&ChainId::ethereum_mainnet())
            .unwrap();
        assert_eq!(mainnet.feeds.len(), 2);
        assert_eq!(
            mainnet.feeds["USDC/USD"].address,
            Address::from_str("0x8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6").unwrap()
        );
        assert_eq!(mainnet.feeds["USDC/USD"].decimals, 8);
        // default value when omitted
        assert_eq!(mainnet.feeds["ETH/USD"].decimals, 8);

        // Pyth
        let pyth = cfg.oracles.pyth.as_ref().unwrap();
        assert_eq!(pyth.endpoint, "https://hermes.pyth.network");
        assert!(pyth.feeds.contains_key("ETH/USD"));

        // Venues
        let hl = cfg.venues.hyperliquid.as_ref().unwrap();
        assert_eq!(hl.endpoint, "https://api.hyperliquid.xyz");
    }

    #[test]
    fn empty_sections_default() {
        let cfg = SyncConfig::load_str("").unwrap();
        assert!(cfg.rpc.chains.is_empty());
        assert!(cfg.oracles.chainlink.chains.is_empty());
        assert!(cfg.oracles.pyth.is_none());
        assert!(cfg.venues.hyperliquid.is_none());
    }

    #[test]
    fn env_var_expansion_in_sync_config() {
        std::env::set_var("TEST_HL_KEY", "hl_secret_42");
        let toml_text = r#"
[venues.hyperliquid]
endpoint = "https://api.hyperliquid.xyz/info?key=${TEST_HL_KEY}"
"#;
        let cfg = SyncConfig::load_str(toml_text).unwrap();
        assert_eq!(
            cfg.venues.hyperliquid.unwrap().endpoint,
            "https://api.hyperliquid.xyz/info?key=hl_secret_42"
        );
    }

    #[test]
    fn parses_multichain_chainlink() {
        let toml_text = r#"
[oracles.chainlink.chains."eip155:1".feeds]
"USDC/USD" = { address = "0x8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6" }

[oracles.chainlink.chains."eip155:42161".feeds]
"USDC/USD" = { address = "0x50834F3163758fcC1Df9973b6e91f0F0F0434aD3" }
"#;
        let cfg = SyncConfig::load_str(toml_text).unwrap();
        let eth = cfg
            .oracles
            .chainlink
            .chains
            .get(&ChainId::ethereum_mainnet())
            .unwrap();
        let arb = cfg
            .oracles
            .chainlink
            .chains
            .get(&ChainId::new("eip155:42161"))
            .unwrap();
        assert_ne!(eth.feeds["USDC/USD"].address, arb.feeds["USDC/USD"].address);
    }
}
