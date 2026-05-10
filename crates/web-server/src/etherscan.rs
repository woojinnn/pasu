//! Etherscan v2 ABI fallback — fourth-tier resolver used when our local
//! tiers (Sourcify curated bundle, SQLite Sourcify dump, openchain selector
//! index) all miss for a given `(chain_id, address, selector)`.
//!
//! - Construction: [`EtherscanClient::from_env`] reads `ETHERSCAN_API_KEY`.
//!   When unset the client is `None`; the orchestrator silently skips this
//!   tier and the request resolves with whatever the local tiers found
//!   (usually `NotFound`).
//! - Lookup: [`EtherscanClient::try_resolve`] checks an in-memory
//!   per-process cache first, then issues a single
//!   `module=contract&action=getabi` call to Etherscan v2 against the
//!   chain id from the request. The full verified ABI is cached so
//!   subsequent selectors against the same `(chain, address)` reuse it.
//! - Negative cache: a `NotVerified` entry is stored when Etherscan reports
//!   the contract isn't verified or the response is malformed, so we don't
//!   hammer the API for repeated unverified lookups within the process
//!   lifetime.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use abi_resolver::decode::{decode_with_function, DecodedCall};
use alloy_json_abi::Function;
use alloy_primitives::Address;
use serde::Deserialize;
use tokio::sync::RwLock;

const ETHERSCAN_V2_BASE: &str = "https://api.etherscan.io/v2/api";
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
enum CacheEntry {
    Found(Arc<Vec<Function>>),
    NotVerified,
}

/// Async client for Etherscan v2's `getabi` endpoint with per-(chain, address)
/// in-memory cache. Construct via [`Self::from_env`]; if no API key is
/// available the constructor returns `None`.
#[derive(Clone)]
pub struct EtherscanClient {
    api_key: String,
    http: reqwest::Client,
    cache: Arc<RwLock<HashMap<(u64, Address), CacheEntry>>>,
}

#[derive(Deserialize)]
struct EtherscanResponse {
    status: String,
    message: String,
    result: serde_json::Value,
}

impl EtherscanClient {
    /// Build a client from `ETHERSCAN_API_KEY`. Returns `None` when the env
    /// var is unset or empty so callers can degrade silently.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ETHERSCAN_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent("scopeball-web-server/0.1")
            .build()
            .ok()?;
        Some(Self {
            api_key: api_key.trim().to_string(),
            http,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Try to decode `calldata` against the verified ABI of `address` on
    /// chain `chain_id`. Returns `None` when the contract is unverified, the
    /// selector isn't in its ABI, or any network/parse step fails.
    pub async fn try_resolve(
        &self,
        chain_id: u64,
        address: &Address,
        calldata: &[u8],
    ) -> Option<DecodedCall> {
        if calldata.len() < 4 {
            return None;
        }
        let key = (chain_id, *address);

        // Cache hit (positive or negative).
        if let Some(entry) = self.cache.read().await.get(&key).cloned() {
            return match entry {
                CacheEntry::Found(funcs) => decode_against(&funcs, calldata),
                CacheEntry::NotVerified => None,
            };
        }

        // Cache miss — fetch.
        match self.fetch_abi(chain_id, address).await {
            Ok(Some(funcs)) => {
                let funcs = Arc::new(funcs);
                self.cache
                    .write()
                    .await
                    .insert(key, CacheEntry::Found(Arc::clone(&funcs)));
                decode_against(&funcs, calldata)
            }
            Ok(None) => {
                self.cache
                    .write()
                    .await
                    .insert(key, CacheEntry::NotVerified);
                None
            }
            Err(e) => {
                tracing::debug!(target: "etherscan", "fetch failed for {address:?} on chain {chain_id}: {e}");
                // Don't cache transient errors so a later retry can succeed.
                None
            }
        }
    }

    async fn fetch_abi(
        &self,
        chain_id: u64,
        address: &Address,
    ) -> Result<Option<Vec<Function>>, reqwest::Error> {
        let url = format!(
            "{ETHERSCAN_V2_BASE}?chainid={chain_id}&module=contract&action=getabi&address=0x{addr}&apikey={key}",
            addr = hex::encode(address.as_slice()),
            key = self.api_key,
        );
        tracing::debug!(target: "etherscan", "fetching ABI: chain={chain_id} address={address:?}");
        let resp: EtherscanResponse = self.http.get(&url).send().await?.json().await?;
        if resp.status != "1" {
            tracing::debug!(target: "etherscan", "not-OK ({}) for {address:?}: {}", resp.status, resp.message);
            return Ok(None);
        }
        // `result` is a JSON-encoded string of an ABI array.
        let abi_str = match resp.result {
            serde_json::Value::String(s) => s,
            _ => return Ok(None),
        };
        let raw_items: Vec<serde_json::Value> = match serde_json::from_str(&abi_str) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(target: "etherscan", "ABI string didn't parse as array for {address:?}: {e}");
                return Ok(None);
            }
        };
        let funcs: Vec<Function> = raw_items
            .into_iter()
            .filter_map(|v| serde_json::from_value::<Function>(v).ok())
            .collect();
        Ok(Some(funcs))
    }
}

fn decode_against(funcs: &[Function], calldata: &[u8]) -> Option<DecodedCall> {
    if calldata.len() < 4 {
        return None;
    }
    let want = [calldata[0], calldata[1], calldata[2], calldata[3]];
    for f in funcs {
        if f.selector().0 == want {
            if let Ok(decoded) = decode_with_function(f, calldata) {
                return Some(decoded);
            }
        }
    }
    None
}
