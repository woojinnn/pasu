//! `CoinGecko` metadata client.
//!
//! Hits `/coins/{platform}/contract/{contract_address}` to grab the
//! token's logo (large), homepage, and short description. Free tier is
//! ~30 req/min without a key; if `COINGECKO_API_KEY` is set, the
//! per-key tier kicks in (Pro plans go higher).
//!
//! This is best-effort metadata for UI rendering — every failure path
//! resolves to `None` so a `CoinGecko` outage doesn't block wallet adds.

use serde::Deserialize;

use policy_state::primitives::{Address, ChainId};
use policy_state::token::TokenMetadata;

const CG_API_BASE: &str = "https://api.coingecko.com/api/v3";

/// Read-mostly `CoinGecko` client. Cheap to clone (`reqwest::Client` is
/// `Arc` internally).
#[derive(Clone, Debug)]
pub struct CoinGeckoClient {
    api_key: Option<String>,
    http: reqwest::Client,
}

impl Default for CoinGeckoClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CoinGeckoClient {
    /// Build a client with no API key (free tier rate limits apply).
    #[must_use]
    pub fn new() -> Self {
        Self {
            api_key: None,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Build with an explicit `x-cg-demo-api-key` (or pro key).
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Read `COINGECKO_API_KEY` from the env. Returns the (possibly
    /// keyless) default client.
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("COINGECKO_API_KEY") {
            Ok(k) if !k.trim().is_empty() => Self::new().with_api_key(k.trim().to_string()),
            _ => Self::new(),
        }
    }

    /// `GET /coins/{platform}/contract/{address}` — token metadata.
    /// Returns `None` for chains `CoinGecko` doesn't index, addresses
    /// `CoinGecko` doesn't know, or transient HTTP errors.
    pub async fn fetch_metadata(&self, chain: &ChainId, address: Address) -> Option<TokenMetadata> {
        let platform = caip2_to_coingecko_platform(chain)?;
        let addr_lower = format!("{address:#x}");
        let url = format!("{CG_API_BASE}/coins/{platform}/contract/{addr_lower}");

        let mut req = self.http.get(&url);
        if let Some(key) = &self.api_key {
            req = req.header("x-cg-demo-api-key", key);
        }
        let resp = req.send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: CgCoinResponse = resp.json().await.ok()?;
        Some(body.into_metadata())
    }
}

/// Map CAIP-2 chain id → `CoinGecko` platform slug. Only the chains the
/// sync config supports today are covered; new chains plug in here.
fn caip2_to_coingecko_platform(chain: &ChainId) -> Option<&'static str> {
    match chain.as_str() {
        "eip155:1" => Some("ethereum"),
        "eip155:42161" => Some("arbitrum-one"),
        "eip155:8453" => Some("base"),
        "eip155:10" => Some("optimistic-ethereum"),
        "eip155:137" => Some("polygon-pos"),
        "eip155:56" => Some("binance-smart-chain"),
        "eip155:43114" => Some("avalanche"),
        _ => None,
    }
}

/// CAIP-2 chain id from a `CoinGecko` platform slug (for the reverse
/// lookup, e.g. when a registry resource points at a `CoinGecko` id).
#[must_use]
pub fn coingecko_platform_to_chain_id(platform: &str) -> Option<ChainId> {
    let raw = match platform {
        "ethereum" => "eip155:1",
        "arbitrum-one" => "eip155:42161",
        "base" => "eip155:8453",
        "optimistic-ethereum" => "eip155:10",
        "polygon-pos" => "eip155:137",
        "binance-smart-chain" => "eip155:56",
        "avalanche" => "eip155:43114",
        _ => return None,
    };
    Some(ChainId::new(raw))
}

// ---------- wire types ----------

#[derive(Debug, Deserialize)]
struct CgCoinResponse {
    id: String,
    #[serde(default)]
    image: CgImage,
    #[serde(default)]
    links: CgLinks,
    #[serde(default)]
    description: CgDescription,
}

#[derive(Debug, Default, Deserialize)]
struct CgImage {
    #[serde(default)]
    large: Option<String>,
    #[serde(default)]
    small: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CgLinks {
    #[serde(default)]
    homepage: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CgDescription {
    #[serde(default)]
    en: Option<String>,
}

impl CgCoinResponse {
    fn into_metadata(self) -> TokenMetadata {
        let logo_url = self
            .image
            .large
            .or(self.image.small)
            .filter(|s| !s.is_empty());
        let website_url = self.links.homepage.into_iter().find(|s| !s.is_empty());
        // Description can be quite long — truncate at 600 chars so the
        // JSON response stays compact. UI re-fetches details on demand.
        let description = self.description.en.filter(|s| !s.is_empty()).map(|s| {
            if s.len() > 600 {
                let mut t = s.chars().take(600).collect::<String>();
                t.push('…');
                t
            } else {
                s
            }
        });
        TokenMetadata {
            logo_url,
            website_url,
            description,
            coingecko_id: Some(self.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_round_trip() {
        for cid in [
            "eip155:1",
            "eip155:42161",
            "eip155:8453",
            "eip155:10",
            "eip155:137",
        ] {
            let p = caip2_to_coingecko_platform(&ChainId::new(cid)).unwrap();
            let back = coingecko_platform_to_chain_id(p).unwrap();
            assert_eq!(back.as_str(), cid);
        }
    }

    #[test]
    fn unknown_chain_returns_none() {
        assert!(caip2_to_coingecko_platform(&ChainId::new("solana:mainnet")).is_none());
    }
}
