pub mod chainlink;
pub mod rest_json;

use async_trait::async_trait;

use policy_state::{DataSource, Decimal};

use crate::error::SyncError;

pub use chainlink::{ChainlinkFeed, ChainlinkFeedRegistry, ChainlinkFetcher};
pub use rest_json::RestJsonOracleFetcher;

#[async_trait]
pub trait PriceFetcher: Send + Sync {
    async fn fetch_price(&self, source: &DataSource) -> Result<Decimal, SyncError>;
}

/// - `Chainlink` → `"chainlink"`
/// - `Pyth`      → `"pyth"`
/// - `RedStone`  → `"redstone"`
#[must_use]
pub fn provider_key(p: &policy_state::OracleProvider) -> String {
    use policy_state::OracleProvider;
    match p {
        OracleProvider::Chainlink => "chainlink".into(),
        OracleProvider::Pyth => "pyth".into(),
        OracleProvider::RedStone => "redstone".into(),
        OracleProvider::Other(s) => s.clone(),
    }
}
