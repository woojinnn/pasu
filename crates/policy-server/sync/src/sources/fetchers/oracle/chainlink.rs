//! ABI: `latestRoundData()` returns
//!   (uint80 roundId, int256 answer, uint256 startedAt, uint256 updatedAt, uint80 answeredInRound)
use std::collections::HashMap;
use std::sync::Arc;

use alloy_primitives::{Address, I256, U256};

use policy_state::{ChainId, DataSource, Decimal};

use crate::config::ChainlinkConfig;
use crate::error::SyncError;
use crate::fetchers::decoder::function_selector;
use crate::fetchers::rpc::{BlockTag, EthCallRequest, RpcRouter};

#[derive(Clone, Debug)]
pub struct ChainlinkFeed {
    pub feed_id: String,
    pub chain: ChainId,
    pub address: Address,
    pub decimals: u8,
}

#[derive(Default)]
pub struct ChainlinkFeedRegistry {
    by_chain_id: HashMap<(ChainId, String), ChainlinkFeed>,
}

impl ChainlinkFeedRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, feed: ChainlinkFeed) {
        self.by_chain_id
            .insert((feed.chain.clone(), feed.feed_id.clone()), feed);
    }

    /// Strict lookup — `(chain, feed_id)` exact match.
    #[must_use]
    pub fn lookup_on(&self, chain: &ChainId, id: &str) -> Option<&ChainlinkFeed> {
        self.by_chain_id.get(&(chain.clone(), id.to_string()))
    }

    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&ChainlinkFeed> {
        self.by_chain_id
            .iter()
            .find(|((_, fid), _)| fid == id)
            .map(|(_, feed)| feed)
    }

    #[must_use]
    pub fn from_config(cfg: &ChainlinkConfig) -> Self {
        let mut r = Self::new();
        for (chain, chain_cfg) in &cfg.chains {
            for (feed_id, feed_cfg) in &chain_cfg.feeds {
                r.register(ChainlinkFeed {
                    feed_id: feed_id.clone(),
                    chain: chain.clone(),
                    address: feed_cfg.address,
                    decimals: feed_cfg.decimals,
                });
            }
        }
        r
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_mainnet_defaults() -> Self {
        use std::str::FromStr;
        let mut r = Self::new();
        let chain = ChainId::ethereum_mainnet();
        let defaults = [
            ("USDC/USD", "0x8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6"),
            ("USDT/USD", "0x3E7d1eAB13ad0104d2750B8863b489D65364e32D"),
            ("ETH/USD", "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419"),
            ("WBTC/USD", "0xF4030086522a5bEEa4988F8cA5B36dbC97BeE88c"),
            ("DAI/USD", "0xAed0c38402a5d19df6E4c03F4E2DceD6e29c1ee9"),
        ];
        for (id, addr) in defaults {
            let address = Address::from_str(addr).unwrap();
            r.register(ChainlinkFeed {
                feed_id: id.into(),
                chain: chain.clone(),
                address,
                decimals: 8,
            });
        }
        r
    }
}

/// Chainlink `AggregatorV3` fetcher.
pub struct ChainlinkFetcher {
    router: Arc<RpcRouter>,
    registry: ChainlinkFeedRegistry,
}

impl ChainlinkFetcher {
    #[must_use]
    pub fn new(router: Arc<RpcRouter>) -> Self {
        Self {
            router,
            registry: ChainlinkFeedRegistry::default(),
        }
    }

    #[must_use]
    pub const fn with_registry(router: Arc<RpcRouter>, registry: ChainlinkFeedRegistry) -> Self {
        Self { router, registry }
    }

    #[must_use]
    pub fn from_sync_config(router: Arc<RpcRouter>, cfg: &ChainlinkConfig) -> Self {
        Self {
            router,
            registry: ChainlinkFeedRegistry::from_config(cfg),
        }
    }

    pub const fn registry_mut(&mut self) -> &mut ChainlinkFeedRegistry {
        &mut self.registry
    }

    pub async fn fetch_price(&self, source: &DataSource) -> Result<Decimal, SyncError> {
        let feed_id = match source {
            DataSource::OracleFeed { feed_id, .. } => feed_id.clone(),
            _ => {
                return Err(SyncError::FetchFailed {
                    source_id: "chainlink".into(),
                    reason: "not an OracleFeed".into(),
                });
            }
        };
        let feed = self
            .registry
            .lookup(&feed_id)
            .ok_or_else(|| SyncError::FetchFailed {
                source_id: "chainlink".into(),
                reason: format!("unknown feed_id: {feed_id}"),
            })?;

        // latestRoundData() selector = first 4 bytes of keccak("latestRoundData()")
        let selector = function_selector("latestRoundData()");
        let req = EthCallRequest::new(feed.address, selector.to_vec());
        let req = EthCallRequest {
            block: BlockTag::Latest,
            ..req
        };
        let data = self.router.eth_call(&feed.chain, req).await?;

        // returndata: 5 × 32 bytes — (roundId, answer, startedAt, updatedAt, answeredInRound)
        if data.len() < 160 {
            return Err(SyncError::FetchFailed {
                source_id: "chainlink".into(),
                reason: format!("latestRoundData returned {} bytes", data.len()),
            });
        }
        let answer = i256_from_be_bytes(&data[32..64])?;
        Ok(scale_to_decimal(answer, feed.decimals))
    }
}

fn i256_from_be_bytes(bytes: &[u8]) -> Result<I256, SyncError> {
    let arr: [u8; 32] = bytes.try_into().map_err(|_| SyncError::FetchFailed {
        source_id: "chainlink".into(),
        reason: "i256 slice not 32 bytes".into(),
    })?;
    Ok(I256::from_be_bytes(arr))
}

fn scale_to_decimal(answer: I256, decimals: u8) -> Decimal {
    let negative = answer.is_negative();
    let mag: U256 = if negative {
        let neg = -answer;
        neg.into_raw()
    } else {
        answer.into_raw()
    };

    let s = mag.to_string();
    let d = decimals as usize;
    let scaled = if s.len() > d {
        let split = s.len() - d;
        format!("{}.{}", &s[..split], &s[split..])
    } else {
        let pad = d - s.len();
        format!("0.{}{}", "0".repeat(pad), s)
    };
    let trimmed = trim_trailing_zeros(&scaled);
    let final_str = if negative {
        format!("-{trimmed}")
    } else {
        trimmed.to_string()
    };
    Decimal::new(final_str)
}

fn trim_trailing_zeros(s: &str) -> &str {
    if !s.contains('.') {
        return s;
    }
    let trimmed = s.trim_end_matches('0');
    trimmed.trim_end_matches('.')
}

#[async_trait::async_trait]
impl crate::fetchers::oracle::PriceFetcher for ChainlinkFetcher {
    async fn fetch_price(&self, source: &DataSource) -> Result<Decimal, SyncError> {
        Self::fetch_price(self, source).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::I256;

    #[test]
    fn scale_8_decimals_positive() {
        // 1.0001 USD with 8 decimals = 100010000
        let answer = I256::try_from(100_010_000_i64).unwrap();
        let d = scale_to_decimal(answer, 8);
        assert_eq!(d.as_str(), "1.0001");
    }

    #[test]
    fn scale_8_decimals_round_value() {
        // 3500 USD with 8 decimals = 350_000_000_000
        let answer = I256::try_from(350_000_000_000_i64).unwrap();
        let d = scale_to_decimal(answer, 8);
        assert_eq!(d.as_str(), "3500");
    }

    #[test]
    fn scale_negative() {
        // -42.5 with 8 decimals = -4_250_000_000
        let answer = I256::try_from(-4_250_000_000_i64).unwrap();
        let d = scale_to_decimal(answer, 8);
        assert_eq!(d.as_str(), "-42.5");
    }

    #[test]
    fn registry_lookup() {
        let r = ChainlinkFeedRegistry::with_mainnet_defaults();
        let feed = r.lookup("USDC/USD").unwrap();
        assert_eq!(feed.decimals, 8);
        assert_eq!(feed.chain, ChainId::ethereum_mainnet());
    }
}
