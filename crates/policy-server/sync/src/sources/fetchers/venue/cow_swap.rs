//! `CoW` Protocol (`CowSwap`) intent-order fetcher.
//!
//! Discovery uses the public Orderbook API, which is **public** (no api-key):
//! `GET https://api.cow.fi/{network}/api/v1/account/{owner}/orders?offset=0&limit=100`.
//! The network is a host-path segment (`mainnet` / `xdai` / `arbitrum_one` /
//! `base` / …), so each configured chain is polled separately and paginated by
//! `offset` until a short page (`< limit`) is returned. HTTP 429 is honored via
//! the `Retry-After` header (one retry, then the page is skipped).
//!
//! Verified against the `CoW` Orderbook `OpenAPI` spec
//! (`github.com/cowprotocol/services` → `crates/orderbook/openapi.yml`):
//! the per-network server URLs, the `/api/v1/account/{owner}/orders` path with
//! `offset`/`limit`, the `Order` field names, and the `OrderStatus` enum
//! (`presignaturePending` / `open` / `fulfilled` / `cancelled` / `expired`).

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;

use policy_state::pending::{
    AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::{DataSource, StateDelta};

use crate::config::CowSwapConfig;
use crate::error::SyncError;
use crate::fetchers::venue::IntentFetcher;

/// Default public Orderbook base (no api-key required). The `{network}` segment
/// is appended per chain by [`cow_network_segment`].
pub const COW_API_BASE: &str = "https://api.cow.fi";

/// Canonical `GPv2Settlement` contract (`PermitCap` spender).
///
/// The same address on every supported chain. This is the spender that pulls
/// `sell` when an order settles, so it is the `PermitCap` spender — it mirrors
/// `project_venue`'s `CoW` → settlement mapping in `effect/amm/intent_order.rs`.
/// The real puller is the `GPv2VaultRelayer`, but `project_venue` records the
/// settlement contract, so we match it.
#[must_use]
pub fn cow_settlement() -> Address {
    Address::from_str("0x9008d19f58aabd9ed0d60971565aa8510560ab41").unwrap_or(Address::ZERO)
}

/// Page size per request (the API caps `limit` at 1000; 100 is a polite poll).
const PAGE_LIMIT: usize = 100;

/// Fetches `CowSwap` order status from the public `CoW` Orderbook API.
pub struct CowSwapFetcher {
    client: reqwest::Client,
    base_url: String,
    chains: Vec<ChainId>,
}

impl CowSwapFetcher {
    #[must_use]
    pub fn from_sync_config(cfg: &CowSwapConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            base_url: cfg.base_url.clone(),
            chains: cfg.chains.clone(),
        }
    }

    /// Poll one chain: paginate `GET {base}/{network}/api/v1/account/{owner}/orders`
    /// by `offset` until a short page is returned, projecting each `Order` into a
    /// `PendingTx`. A page that fails to fetch ends the loop for that chain (the
    /// orders already collected are still returned).
    async fn fetch_chain(
        &self,
        chain: &ChainId,
        swapper: &Address,
        now: Time,
    ) -> Result<Vec<PendingTx>, SyncError> {
        let Some(network) = cow_network_segment(chain) else {
            return Err(SyncError::FetchFailed {
                source_id: "cow_swap".into(),
                reason: format!("unsupported chain {}", chain.as_str()),
            });
        };
        let mut out = Vec::new();
        let mut offset = 0usize;
        loop {
            let url = format!(
                "{}/{network}/api/v1/account/{:#x}/orders?offset={offset}&limit={PAGE_LIMIT}",
                self.base_url.trim_end_matches('/'),
                swapper,
            );
            let body = self.get_with_retry(&url).await?;
            let Some(orders) = parse_orders(&body) else {
                break;
            };
            let page_len = orders.len();
            for o in &orders {
                out.push(o.to_pending_tx(chain, now));
            }
            if page_len < PAGE_LIMIT {
                break;
            }
            offset += PAGE_LIMIT;
        }
        Ok(out)
    }

    /// `GET` honoring a single `Retry-After` on HTTP 429.
    async fn get_with_retry(&self, url: &str) -> Result<serde_json::Value, SyncError> {
        let resp = self.send(url).await?;
        if resp.status().as_u16() == 429 {
            let wait = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(1)
                .min(30);
            tokio::time::sleep(Duration::from_secs(wait)).await;
            let resp = self.send(url).await?;
            return Self::decode(resp).await;
        }
        Self::decode(resp).await
    }

    async fn send(&self, url: &str) -> Result<reqwest::Response, SyncError> {
        self.client
            .get(url)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "cow_swap".into(),
                reason: format!("http: {e}"),
            })
    }

    async fn decode(resp: reqwest::Response) -> Result<serde_json::Value, SyncError> {
        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "cow_swap".into(),
                reason: format!("status {}", resp.status()),
            });
        }
        resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "cow_swap".into(),
            reason: format!("decode: {e}"),
        })
    }
}

#[async_trait]
impl IntentFetcher for CowSwapFetcher {
    async fn fetch_orders(
        &self,
        swapper: &Address,
        now: Time,
    ) -> Result<Vec<PendingTx>, SyncError> {
        let mut out = Vec::new();
        let mut last_err = None;
        for chain in &self.chains {
            match self.fetch_chain(chain, swapper, now).await {
                Ok(mut orders) => out.append(&mut orders),
                Err(e) => last_err = Some(e),
            }
        }
        // Surface a per-chain failure only when nothing at all came back, so one
        // dead chain doesn't drop healthy chains' orders.
        if out.is_empty() {
            if let Some(e) = last_err {
                return Err(e);
            }
        }
        Ok(out)
    }
}

/// Map a CAIP-2 chain to the `CoW` Orderbook host-path segment. Returns `None`
/// for chains `CoW` does not serve. Segments verified against the `OpenAPI`
/// `servers` list.
#[must_use]
pub fn cow_network_segment(chain: &ChainId) -> Option<&'static str> {
    match chain.as_str() {
        "eip155:1" => Some("mainnet"),
        "eip155:100" => Some("xdai"),
        "eip155:42161" => Some("arbitrum_one"),
        "eip155:8453" => Some("base"),
        "eip155:137" => Some("polygon"),
        "eip155:59144" => Some("linea"),
        "eip155:56" => Some("bnb"),
        "eip155:43114" => Some("avalanche"),
        _ => None,
    }
}

/// Map a `CoW` `status` string to our canonical `PendingStatus`.
///
/// `CoW` keeps `status: "open"` while an order is partially executed, so the
/// partial-fill edge is detected by amount comparison
/// (`0 < executed_sell < sell_amount`), not by the status string. The second
/// tuple element is the verbatim venue string, stored in
/// `PendingLifecycle.raw_status`.
#[must_use]
pub fn map_cowswap_status(
    raw: &str,
    executed_sell: U256,
    sell_amount: U256,
) -> (PendingStatus, Option<String>) {
    let status = match raw {
        "open" | "presignaturePending" => {
            if executed_sell > U256::ZERO && executed_sell < sell_amount {
                PendingStatus::PartiallyFilled
            } else {
                PendingStatus::Active
            }
        }
        "fulfilled" => PendingStatus::Filled,
        "cancelled" => PendingStatus::Cancelled,
        "expired" => PendingStatus::Expired,
        _ => PendingStatus::Unknown,
    };
    (status, Some(raw.to_owned()))
}

/// One `CoW` `Order`. Only the fields we project are decoded; unknown fields
/// are ignored (defensive serde — the API evolves).
#[derive(Debug, Clone)]
pub struct CowSwapOrder {
    pub uid: String,
    pub status: String,
    pub valid_to: Option<u64>,
    pub sell_token: Address,
    pub buy_token: Address,
    pub sell_amount: U256,
    pub buy_min: U256,
    pub executed_sell: U256,
}

/// Decode the top-level `Order[]` array. The account-orders endpoint returns a
/// bare JSON array (not an envelope). Elements lacking a decodable
/// `uid` / `status` / `sellToken` / `buyToken` are skipped.
#[must_use]
pub fn parse_orders(value: &serde_json::Value) -> Option<Vec<CowSwapOrder>> {
    let arr = value.as_array()?;
    Some(arr.iter().filter_map(parse_one).collect())
}

fn parse_one(o: &serde_json::Value) -> Option<CowSwapOrder> {
    let uid = o.get("uid")?.as_str()?.to_owned();
    let status = o.get("status")?.as_str()?.to_owned();
    let valid_to = o.get("validTo").and_then(serde_json::Value::as_u64);
    let sell_token = Address::from_str(o.get("sellToken")?.as_str()?).ok()?;
    let buy_token = Address::from_str(o.get("buyToken")?.as_str()?).ok()?;
    let sell_amount = amount(o, "sellAmount");
    let buy_min = amount(o, "buyAmount");
    let executed_sell = amount(o, "executedSellAmount");
    Some(CowSwapOrder {
        uid,
        status,
        valid_to,
        sell_token,
        buy_token,
        sell_amount,
        buy_min,
        executed_sell,
    })
}

/// Read a decimal-string atom amount field; `0` when absent or unparseable
/// (`CoW` amounts are venue-supplied decimal strings).
fn amount(o: &serde_json::Value, key: &str) -> U256 {
    o.get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(|s| U256::from_str(s).ok())
        .unwrap_or(U256::ZERO)
}

fn token_ref(chain: &ChainId, address: Address) -> TokenRef {
    // CoW uses the 0xeee sentinel for native ETH (buy side).
    let native =
        Address::from_str("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap_or(Address::ZERO);
    if address == Address::ZERO || address == native {
        TokenRef {
            key: TokenKey::Native {
                chain: chain.clone(),
            },
        }
    } else {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address,
            },
        }
    }
}

impl CowSwapOrder {
    /// Project this order into a `PendingTx`. The chain comes from the polled
    /// chain (`CoW` orders are per-network). The `uid` is embedded in the id so
    /// upserts are idempotent.
    #[must_use]
    pub fn to_pending_tx(&self, chain: &ChainId, now: Time) -> PendingTx {
        let (status, raw_status) =
            map_cowswap_status(&self.status, self.executed_sell, self.sell_amount);
        let sell = token_ref(chain, self.sell_token);
        let buy = token_ref(chain, self.buy_token);

        PendingTx {
            id: format!("intent:cow_swap:{}", self.uid),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef {
                    name: "cow_swap".into(),
                    chain: Some(chain.clone()),
                },
                sell: sell.clone(),
                buy,
                sell_max: self.sell_amount,
                buy_min: self.buy_min,
                // CoW orders are limit orders (a fixed buy/sell price).
                order_kind: OrderKind::Limit,
            },
            commitment: AssetCommitment::PermitCap {
                token: sell,
                spender: cow_settlement(),
                max_out: self.sell_amount,
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status,
                valid_until: self.valid_to.map(Time::from_unix),
                nonce: None, // uid lives in `id`
                on_chain_tx: None,
                raw_status,
            },
            sync: DataSource::VenueApi {
                endpoint: COW_API_BASE.into(),
                parser_id: "cow_swap_orders".into(),
                auth: None,
            },
            signed_at: now,
            signature_payload: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Realistic CoW account-orders response (bare array). One open order partly
    // executed (partial fill), one fully fulfilled.
    const ORDERS: &str = r#"[
      {
        "uid": "0xaaa111",
        "status": "open",
        "validTo": 1738003600,
        "creationDate": "2025-01-27T00:00:00.000Z",
        "sellToken": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        "buyToken": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "sellAmount": "1000000000",
        "buyAmount": "300000000000000000",
        "executedSellAmount": "400000000",
        "executedBuyAmount": "120000000000000000",
        "invalidated": false
      },
      {
        "uid": "0xbbb222",
        "status": "fulfilled",
        "validTo": 1738003600,
        "creationDate": "2025-01-27T00:00:00.000Z",
        "sellToken": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        "buyToken": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "sellAmount": "500000000",
        "buyAmount": "100000000000000000",
        "executedSellAmount": "500000000",
        "executedBuyAmount": "101000000000000000",
        "invalidated": false
      }
    ]"#;

    #[test]
    fn status_mapping_covers_all_enum_values_and_partial_fill_edge() {
        let sell = U256::from(1_000u64);
        // open, no execution → Active.
        assert_eq!(
            map_cowswap_status("open", U256::ZERO, sell).0,
            PendingStatus::Active
        );
        // open, partially executed → PartiallyFilled.
        assert_eq!(
            map_cowswap_status("open", U256::from(400u64), sell).0,
            PendingStatus::PartiallyFilled
        );
        // open, fully executed (executed == sell) → Active, not PartiallyFilled
        // (status fulfilled comes separately; equality is not "partial").
        assert_eq!(
            map_cowswap_status("open", sell, sell).0,
            PendingStatus::Active
        );
        // presignaturePending behaves like open.
        assert_eq!(
            map_cowswap_status("presignaturePending", U256::ZERO, sell).0,
            PendingStatus::Active
        );
        assert_eq!(
            map_cowswap_status("presignaturePending", U256::from(1u64), sell).0,
            PendingStatus::PartiallyFilled
        );
        assert_eq!(
            map_cowswap_status("fulfilled", sell, sell).0,
            PendingStatus::Filled
        );
        assert_eq!(
            map_cowswap_status("cancelled", U256::ZERO, sell).0,
            PendingStatus::Cancelled
        );
        assert_eq!(
            map_cowswap_status("expired", U256::ZERO, sell).0,
            PendingStatus::Expired
        );
        assert_eq!(
            map_cowswap_status("something-new", U256::ZERO, sell).0,
            PendingStatus::Unknown
        );
        // raw preserved.
        assert_eq!(
            map_cowswap_status("open", U256::ZERO, sell).1.as_deref(),
            Some("open")
        );
    }

    #[test]
    fn parses_order_into_pending_tx() {
        let value: serde_json::Value = serde_json::from_str(ORDERS).unwrap();
        let orders = parse_orders(&value).unwrap();
        assert_eq!(orders.len(), 2);

        let p =
            orders[0].to_pending_tx(&ChainId::ethereum_mainnet(), Time::from_unix(1_738_000_000));
        assert_eq!(p.id, "intent:cow_swap:0xaaa111");
        // Partly executed open order → PartiallyFilled.
        assert_eq!(p.lifecycle.status, PendingStatus::PartiallyFilled);
        assert_eq!(
            p.lifecycle.valid_until,
            Some(Time::from_unix(1_738_003_600))
        );
        assert_eq!(p.lifecycle.raw_status.as_deref(), Some("open"));
        match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue,
                sell_max,
                buy_min,
                order_kind,
                ..
            } => {
                assert_eq!(venue.name, "cow_swap");
                assert_eq!(venue.chain, Some(ChainId::ethereum_mainnet()));
                assert_eq!(*order_kind, OrderKind::Limit);
                assert_eq!(*sell_max, U256::from(1_000_000_000u64));
                assert_eq!(*buy_min, U256::from(300_000_000_000_000_000u64));
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
        // Spender is the GPv2Settlement contract.
        match &p.commitment {
            AssetCommitment::PermitCap { spender, .. } => assert_eq!(*spender, cow_settlement()),
            other => panic!("expected PermitCap, got {other:?}"),
        }

        // Second order is fully fulfilled → Filled.
        let p2 =
            orders[1].to_pending_tx(&ChainId::ethereum_mainnet(), Time::from_unix(1_738_000_000));
        assert_eq!(p2.id, "intent:cow_swap:0xbbb222");
        assert_eq!(p2.lifecycle.status, PendingStatus::Filled);
    }

    #[test]
    fn network_segments_map_known_chains() {
        assert_eq!(
            cow_network_segment(&ChainId::ethereum_mainnet()),
            Some("mainnet")
        );
        assert_eq!(
            cow_network_segment(&ChainId::arbitrum()),
            Some("arbitrum_one")
        );
        assert_eq!(cow_network_segment(&ChainId::base()), Some("base"));
        assert_eq!(
            cow_network_segment(&ChainId::new("eip155:100")),
            Some("xdai")
        );
        assert_eq!(cow_network_segment(&ChainId::new("eip155:999999")), None);
    }
}
