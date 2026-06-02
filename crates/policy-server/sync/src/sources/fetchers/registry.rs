use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::RwLock;

use policy_state::{DataSource, RegistryResource};

use crate::error::SyncError;

/// Cache TTL (default 24h).
const DEFAULT_CACHE_TTL: Duration = Duration::from_hours(24);

#[derive(Clone, Debug)]
struct CacheEntry {
    value: Value,
    inserted_at: Instant,
}

/// Registry fetcher with in-memory TTL cache.
pub struct RegistryFetcher {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    cache_ttl: Duration,
}

impl Default for RegistryFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: DEFAULT_CACHE_TTL,
        }
    }

    #[must_use]
    pub const fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    pub async fn fetch(&self, source: &DataSource) -> Result<Value, SyncError> {
        let (endpoint, resource, version) = match source {
            DataSource::RegistryApi {
                endpoint,
                resource,
                version,
            } => (endpoint.clone(), resource.clone(), version.clone()),
            _ => {
                return Err(SyncError::FetchFailed {
                    source_id: "registry".into(),
                    reason: "not a RegistryApi source".into(),
                });
            }
        };

        let url = build_url(&endpoint, &resource, version.as_deref());
        let cache_key = url.clone();

        // Cache hit?
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                if entry.inserted_at.elapsed() < self.cache_ttl {
                    return Ok(entry.value.clone());
                }
            }
        }

        // Fetch.
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "registry".into(),
                reason: format!("http: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "registry".into(),
                reason: format!("status {}: {}", resp.status(), url),
            });
        }

        let value: Value = resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "registry".into(),
            reason: format!("json decode: {e}"),
        })?;

        // Cache.
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                cache_key,
                CacheEntry {
                    value: value.clone(),
                    inserted_at: Instant::now(),
                },
            );
        }

        Ok(value)
    }

    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }

    pub async fn cache_size(&self) -> usize {
        self.cache.read().await.len()
    }
}

fn build_url(endpoint: &str, resource: &RegistryResource, version: Option<&str>) -> String {
    let base = endpoint.trim_end_matches('/');
    let v = version.unwrap_or("v1");
    match resource {
        RegistryResource::TokenMeta { chain, address } => {
            format!("{base}/{v}/token/{chain}/{address:#x}")
        }
        RegistryResource::ProtocolMap { chain, address } => {
            format!("{base}/{v}/protocol/{chain}/{address:#x}")
        }
        RegistryResource::PoolMeta { chain, pool_addr } => {
            format!("{base}/{v}/pool/{chain}/{pool_addr:#x}")
        }
        RegistryResource::DecoderRegistry => {
            format!("{base}/{v}/decoder")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{Address, ChainId};
    use std::str::FromStr;

    #[test]
    fn builds_token_url() {
        let addr = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
        let url = build_url(
            "http://localhost:8080/",
            &RegistryResource::TokenMeta {
                chain: ChainId::ethereum_mainnet(),
                address: addr,
            },
            None,
        );
        assert!(url.starts_with("http://localhost:8080/v1/token/eip155:1/0xa0b86991"));
    }

    #[test]
    fn builds_with_explicit_version() {
        let addr = Address::ZERO;
        let url = build_url(
            "http://localhost:8080",
            &RegistryResource::ProtocolMap {
                chain: ChainId::base(),
                address: addr,
            },
            Some("v2"),
        );
        assert_eq!(
            url,
            "http://localhost:8080/v2/protocol/eip155:8453/0x0000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn builds_decoder_url() {
        let url = build_url(
            "http://localhost:8080",
            &RegistryResource::DecoderRegistry,
            None,
        );
        assert_eq!(url, "http://localhost:8080/v1/decoder");
    }
}
