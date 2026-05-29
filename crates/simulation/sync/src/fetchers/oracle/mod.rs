//! Oracle provider 별 구현.
//!
//! 공통 인터페이스 [`PriceFetcher`] 를 두고, on-chain (Chainlink, RedStone proxy 등)
//! 과 off-chain REST ([`RestJsonOracleFetcher`] — CoinGecko / Pyth Hermes /
//! CoinMarketCap 등) 가 모두 같은 trait 을 impl 한다.
//!
//! Orchestrator 는 `HashMap<provider_name, Arc<dyn PriceFetcher>>` 로 dispatch
//! 하므로, 새 REST provider 추가는 `scopeball-sync.toml` 의
//! `[oracles.rest.<name>]` 한 블록 만으로 충분 (Rust 코드 변경 0).

pub mod chainlink;
pub mod rest_json;

use async_trait::async_trait;

use simulation_state::{DataSource, Decimal};

use crate::error::SyncError;

pub use chainlink::{ChainlinkFeed, ChainlinkFeedRegistry, ChainlinkFetcher};
pub use rest_json::RestJsonOracleFetcher;

/// 가격 oracle 의 공통 trait.
///
/// 입력 `DataSource` 는 항상 [`DataSource::OracleFeed`] 여야 한다 — 다른 variant
/// 면 [`SyncError::FetchFailed`] 로 거절. 반환값은 USD-quoted decimal 가격.
#[async_trait]
pub trait PriceFetcher: Send + Sync {
    async fn fetch_price(&self, source: &DataSource) -> Result<Decimal, SyncError>;
}

/// `OracleProvider` enum 을 dispatch HashMap 의 키로 쓸 수 있도록 정규화한다.
///
/// - `Chainlink` → `"chainlink"`
/// - `Pyth`      → `"pyth"`
/// - `RedStone`  → `"redstone"`
/// - `Other(s)`  → `s` (그대로)
///
/// `[oracles.rest.<name>]` 의 `<name>` 도 같은 규칙 따라야 매칭됨.
#[must_use]
pub fn provider_key(p: &simulation_state::OracleProvider) -> String {
    use simulation_state::OracleProvider;
    match p {
        OracleProvider::Chainlink => "chainlink".into(),
        OracleProvider::Pyth => "pyth".into(),
        OracleProvider::RedStone => "redstone".into(),
        OracleProvider::Other(s) => s.clone(),
    }
}
