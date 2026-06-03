//! Runtime sync configuration loaded from `scopeball-sync.toml`.
//!
//! The file configures RPC failover, oracle catalogs, and venue API endpoints.
//! Environment variable references in the form `${VAR}` are expanded before
//! TOML parsing, which keeps secrets out of checked-in config.
//!
//! ```toml
//! [rpc.failover]
//! strategy = "priority"
//! [rpc.chains."eip155:1"]
//! multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"
//! [[rpc.chains."eip155:1".providers]]
//! name = "publicnode"
//! kind = "public"
//! url  = "https://ethereum-rpc.publicnode.com"
//! priority = 1
//! [oracles.chainlink.chains."eip155:1".feeds]
//! "USDC/USD" = { address = "0x8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6", decimals = 8 }
//! [oracles.pyth]
//! endpoint = "https://hermes.pyth.network"
//! [oracles.pyth.feeds]
//! "ETH/USD" = { price_id = "0xff61491a..." }
//! [venues.hyperliquid]
//! endpoint = "https://api.hyperliquid.xyz"
//! ```
use std::collections::BTreeMap;
use std::path::Path;

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

use policy_state::ChainId;

use crate::error::SyncError;
use crate::fetchers::rpc::RpcConfig;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// RPC providers + failover.
    #[serde(default)]
    pub rpc: RpcConfig,

    /// Oracle feed catalogs.
    #[serde(default)]
    pub oracles: OraclesConfig,

    /// Venue API endpoints.
    #[serde(default)]
    pub venues: VenuesConfig,
}

impl SyncConfig {
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OraclesConfig {
    /// Chainlink `AggregatorV3` feed catalog.
    #[serde(default)]
    pub chainlink: ChainlinkConfig,

    /// Optional Pyth Hermes REST configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pyth: Option<PythConfig>,

    /// Generic REST JSON oracle providers keyed by canonical provider name.
    #[serde(default)]
    pub rest: BTreeMap<String, RestOracleConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChainlinkConfig {
    /// Per-chain feed catalogs.
    #[serde(default)]
    pub chains: BTreeMap<ChainId, ChainlinkChainConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChainlinkChainConfig {
    /// Feed metadata keyed by feed id, for example `USDC/USD`.
    #[serde(default)]
    pub feeds: BTreeMap<String, ChainlinkFeedConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainlinkFeedConfig {
    /// `AggregatorV3` contract address.
    pub address: Address,
    /// Aggregator decimals; Chainlink USD feeds usually use 8.
    #[serde(default = "default_chainlink_decimals")]
    pub decimals: u8,
}

const fn default_chainlink_decimals() -> u8 {
    8
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PythConfig {
    /// Hermes base URL.
    pub endpoint: String,
    #[serde(default)]
    pub feeds: BTreeMap<String, PythFeedConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PythFeedConfig {
    /// Pyth price feed id, "0x" + 64 hex.
    pub price_id: String,
}

// ---------------------------------------------------------------------------
// Generic REST JSON oracle
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestOracleConfig {
    pub base_url: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<RestAuthConfig>,

    #[serde(default = "default_rest_timeout_sec")]
    pub timeout_sec: u64,

    #[serde(default)]
    pub feeds: BTreeMap<String, RestFeedConfig>,
}

const fn default_rest_timeout_sec() -> u64 {
    10
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestAuthConfig {
    pub header_name: String,
    pub env_var: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestFeedConfig {
    pub path: String,

    pub json_pointer: String,
}

// ---------------------------------------------------------------------------
// Venues
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VenuesConfig {
    /// Hyperliquid REST API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyperliquid: Option<HyperliquidConfig>,
    /// Uniswap Trade API (UniswapX order status).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uniswap: Option<UniswapConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HyperliquidConfig {
    pub endpoint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UniswapConfig {
    /// Base URL for the Trade API, e.g. `https://trade-api.gateway.uniswap.org/v1`.
    pub orders_endpoint: String,
    /// `x-api-key` value. `${VAR}` is expanded from the environment by
    /// `SyncConfig::load_*` before this struct is built.
    pub api_key: String,
    /// Chains to poll (CAIP-2). The numeric `chainId` query param is derived
    /// from each entry's `eip155:<n>` suffix.
    #[serde(default)]
    pub chains: Vec<ChainId>,
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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
    fn parses_uniswap_venue_with_env_key() {
        std::env::set_var("TEST_UNISWAP_KEY", "uni_secret_7");
        let toml_text = r#"
[venues.uniswap]
orders_endpoint = "https://trade-api.gateway.uniswap.org/v1"
api_key = "${TEST_UNISWAP_KEY}"
chains = ["eip155:1"]
"#;
        let cfg = SyncConfig::load_str(toml_text).unwrap();
        let uni = cfg.venues.uniswap.as_ref().unwrap();
        assert_eq!(
            uni.orders_endpoint,
            "https://trade-api.gateway.uniswap.org/v1"
        );
        assert_eq!(uni.api_key, "uni_secret_7");
        assert_eq!(uni.chains, vec![ChainId::ethereum_mainnet()]);
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
