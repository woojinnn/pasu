//! 1inch Fusion+ (**cross-chain**) intent-order fetcher.
//!
//! Discovery uses the 1inch Dev Portal Fusion+ Orders API, which **requires** an
//! `Authorization: Bearer <key>` header (the same Dev Portal key as same-chain
//! Fusion):
//! `GET https://api.1inch.dev/fusion-plus/orders/v1.2/order/maker/{address}/?page=1&limit=100`.
//!
//! Unlike same-chain Fusion, the chain is **not** in the path — one query returns
//! the maker's orders across all chains, each carrying its own `srcChainId` /
//! `dstChainId`. The order sells on the source chain and buys on the destination
//! chain, so the projected `OffchainLimitOrder` carries `sell` on `srcChain` and
//! `buy` on `dstChain` (a `TokenKey` carries its own chain). Non-EVM chains
//! (Solana, network id 501) are skipped — they are not `eip155:` and carry a
//! different order shape.
//!
//! The by-maker endpoint is **history-inclusive** (it returns terminal orders —
//! `executed` / `cancelled` / `refunded` — alongside `pending` ones; verified
//! against `1inch/cross-chain-sdk` `OrderFillsByMakerOutput` + its order-api
//! spec test, which mixes a `Pending` and an `Executed` item in one response,
//! and a separate `order/active` endpoint exists). So terminal transitions are
//! observed directly and pruned by `upsert_intent_orders`. We still expose an
//! `authoritative_prefix` so that any order which ages off the list entirely
//! (the Dev Portal's retention window is undocumented) is snapshot-pruned rather
//! than lingering as a stale `Active` entry. For that prune to be safe the fetch
//! is fail-closed: `fetch_orders` returns `Err` on any incompleteness (transport/
//! non-2xx/decode via `?`, an unrecognized 200 body shape, or `MAX_PAGES`
//! exhaustion) and sizes pagination by the RAW page length, so it never returns a
//! truncated `Ok` that the snapshot-prune would mistake for the complete set.

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;

use policy_state::pending::{
    AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::{DataSource, StateDelta};

use crate::config::OneInchFusionPlusConfig;
use crate::error::SyncError;
use crate::fetchers::venue::IntentFetcher;

/// Default Dev Portal Fusion+ base. The `/orders/v1.2/...` segments are appended.
pub const FUSION_PLUS_API_BASE: &str = "https://api.1inch.dev/fusion-plus";

/// `PendingTx` id prefix for this venue. Also the `authoritative_prefix`.
pub const FUSION_PLUS_PREFIX: &str = "intent:one_inch_fusion_plus:";

/// Solana network id in the 1inch cross-chain SDK — non-EVM, not `eip155:`.
const SOLANA_NETWORK_ID: u64 = 501;

/// Page size per request.
const PAGE_LIMIT: usize = 100;

/// Pagination safety bound (never expect this many pages for one maker).
const MAX_PAGES: usize = 1000;

/// Fetches 1inch Fusion+ cross-chain order status from the Dev Portal.
pub struct OneInchFusionPlusFetcher {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OneInchFusionPlusFetcher {
    #[must_use]
    pub fn from_sync_config(cfg: &OneInchFusionPlusConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            base_url: cfg.base_url.clone(),
            api_key: cfg.api_key.clone(),
        }
    }

    async fn get(&self, url: &str) -> Result<serde_json::Value, SyncError> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "one_inch_fusion_plus".into(),
                reason: format!("http: {e}"),
            })?;
        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "one_inch_fusion_plus".into(),
                reason: format!("status {}", resp.status()),
            });
        }
        resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "one_inch_fusion_plus".into(),
            reason: format!("decode: {e}"),
        })
    }
}

#[async_trait]
impl IntentFetcher for OneInchFusionPlusFetcher {
    /// Paginate the by-maker endpoint until a short page; project each order.
    ///
    /// Fail-closed for snapshot-prune safety — returns `Err` on ANY incompleteness
    /// (never a partial `Ok`): a transport/non-2xx/decode error via `?`, a 200
    /// whose body is an unrecognized shape (`parse_orders` → `None`), or
    /// `MAX_PAGES` exhaustion. The short-page terminator uses the RAW element
    /// count (not the count after dropping undecodable items / non-EVM legs), so a
    /// single bad item on a full page can never truncate the walk.
    async fn fetch_orders(
        &self,
        swapper: &Address,
        now: Time,
    ) -> Result<Vec<PendingTx>, SyncError> {
        let mut out = Vec::new();
        for page in 1..=MAX_PAGES {
            let url = format!(
                "{}/orders/v1.2/order/maker/{:#x}/?page={page}&limit={PAGE_LIMIT}",
                self.base_url.trim_end_matches('/'),
                swapper,
            );
            let body = self.get(&url).await?;
            let raw_len = raw_page_len(&body).ok_or_else(|| SyncError::FetchFailed {
                source_id: "one_inch_fusion_plus".into(),
                reason: "unrecognized response shape (not an array or {items:[...]})".into(),
            })?;
            for o in &parse_orders(&body).unwrap_or_default() {
                if let Some(p) = o.to_pending_tx(now) {
                    out.push(p);
                }
            }
            if raw_len < PAGE_LIMIT {
                return Ok(out);
            }
        }
        Err(SyncError::FetchFailed {
            source_id: "one_inch_fusion_plus".into(),
            reason: format!("order walk exceeded {MAX_PAGES} pages; treating as incomplete"),
        })
    }

    fn authoritative_prefix(&self) -> Option<&str> {
        Some(FUSION_PLUS_PREFIX)
    }
}

/// `<n>` → `eip155:<n>` `ChainId`. `None` for non-EVM ids (Solana 501) so the
/// order is skipped rather than mislabelled as `eip155:501`.
#[must_use]
pub fn evm_chain_from_numeric(n: u64) -> Option<ChainId> {
    if n == SOLANA_NETWORK_ID {
        return None;
    }
    Some(ChainId::new(format!("eip155:{n}")))
}

/// Map a Fusion+ `status` (+ optional `validation`) to our `PendingStatus`.
///
/// The cross-chain `status` enum is `pending` / `executed` / `expired` /
/// `cancelled` / `refunding` / `refunded` / `unpublished`. `refunding` /
/// `refunded` are cross-chain-specific terminal-ish states (escrow timed out,
/// funds returned to maker) → `Failed`. The `validation` field is recorded
/// verbatim in `raw_status` (so an un-fillable reason is visible to Phase 4) but
/// does NOT itself drive the lifecycle status, to avoid prematurely retiring a
/// `pending` order that is only temporarily invalid.
#[must_use]
pub fn map_one_inch_fusion_plus_status(
    status: &str,
    validation: Option<&str>,
) -> (PendingStatus, Option<String>) {
    let mapped = match status {
        "pending" | "unpublished" => PendingStatus::Active,
        "executed" => PendingStatus::Filled,
        "expired" => PendingStatus::Expired,
        "cancelled" => PendingStatus::Cancelled,
        "refunding" | "refunded" => PendingStatus::Failed,
        _ => PendingStatus::Unknown,
    };
    let raw = match validation {
        Some(v) => format!("{status}/{v}"),
        None => status.to_owned(),
    };
    (mapped, Some(raw))
}

/// One Fusion+ order (`OrderFillsByMakerOutput`). Only projected fields decoded.
#[derive(Debug, Clone)]
pub struct OneInchFusionPlusOrder {
    pub order_hash: String,
    pub status: String,
    pub validation: Option<String>,
    pub maker_asset: Address,
    pub taker_asset: Address,
    pub maker_amount: U256,
    pub min_taker_amount: U256,
    pub src_chain_id: u64,
    pub dst_chain_id: u64,
    pub auction_start: Option<u64>,
    pub auction_duration: Option<u64>,
}

/// The list array of the Fusion+ response (a bare array or the `items` array).
/// `None` for an unrecognized envelope (used both to fail closed and to size
/// pagination by the RAW element count, before any defensive item drops).
fn list_array(value: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    value
        .as_array()
        .or_else(|| value.get("items").and_then(serde_json::Value::as_array))
}

/// Raw element count of the list page (pre-filter). `None` if the body is an
/// unrecognized shape — the caller treats that as an incomplete fetch (`Err`).
#[must_use]
pub fn raw_page_len(value: &serde_json::Value) -> Option<usize> {
    list_array(value).map(Vec::len)
}

/// Decode the Fusion+ list response (bare array or `{ "items": [...] }`).
///
/// Elements lacking a decodable `orderHash` / `status` / assets / chain ids are
/// skipped (defensive serde). `None` only for an unrecognized envelope.
#[must_use]
pub fn parse_orders(value: &serde_json::Value) -> Option<Vec<OneInchFusionPlusOrder>> {
    Some(list_array(value)?.iter().filter_map(parse_one).collect())
}

fn parse_one(o: &serde_json::Value) -> Option<OneInchFusionPlusOrder> {
    let order_hash = o.get("orderHash")?.as_str()?.to_owned();
    let status = o.get("status")?.as_str()?.to_owned();
    let validation = o
        .get("validation")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let maker_asset = Address::from_str(o.get("makerAsset")?.as_str()?).ok()?;
    let taker_asset = Address::from_str(o.get("takerAsset")?.as_str()?).ok()?;
    let maker_amount = amount(o, "makerAmount");
    let min_taker_amount = amount(o, "minTakerAmount");
    let src_chain_id = u64_field(o, "srcChainId")?;
    let dst_chain_id = u64_field(o, "dstChainId")?;
    let auction_start = u64_field(o, "auctionStartDate");
    let auction_duration = u64_field(o, "auctionDuration");
    Some(OneInchFusionPlusOrder {
        order_hash,
        status,
        validation,
        maker_asset,
        taker_asset,
        maker_amount,
        min_taker_amount,
        src_chain_id,
        dst_chain_id,
        auction_start,
        auction_duration,
    })
}

fn amount(o: &serde_json::Value, key: &str) -> U256 {
    o.get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(|s| U256::from_str(s).ok())
        .or_else(|| {
            o.get(key)
                .and_then(serde_json::Value::as_u64)
                .map(U256::from)
        })
        .unwrap_or(U256::ZERO)
}

fn u64_field(o: &serde_json::Value, key: &str) -> Option<u64> {
    let v = o.get(key)?;
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn token_ref(chain: &ChainId, address: Address) -> TokenRef {
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

impl OneInchFusionPlusOrder {
    /// `auctionStartDate + auctionDuration` (both unix seconds) as the expiry.
    #[must_use]
    pub fn expiry(&self) -> Option<Time> {
        match (self.auction_start, self.auction_duration) {
            (Some(s), Some(d)) => Some(Time::from_unix(s.saturating_add(d))),
            _ => None,
        }
    }

    /// Project into a `PendingTx`. Returns `None` when either side is a non-EVM
    /// chain (the order cannot be modelled as `eip155:` token keys).
    ///
    /// The spend happens on the source chain, so `venue.chain` and the `PermitCap`
    /// `token` are the `sell` (src) side; `buy` is on the destination chain. The
    /// `PermitCap` `spender` is `Address::ZERO` for v1 (same-chain Fusion
    /// convention); the real src-chain spender is the per-order escrow contract —
    /// resolving it is deferred to Phase 4.
    #[must_use]
    pub fn to_pending_tx(&self, now: Time) -> Option<PendingTx> {
        let src_chain = evm_chain_from_numeric(self.src_chain_id)?;
        let dst_chain = evm_chain_from_numeric(self.dst_chain_id)?;
        let expiry = self.expiry();
        let (status, raw_status) =
            map_one_inch_fusion_plus_status(&self.status, self.validation.as_deref());
        let sell = token_ref(&src_chain, self.maker_asset);
        let buy = token_ref(&dst_chain, self.taker_asset);

        Some(PendingTx {
            id: format!("{FUSION_PLUS_PREFIX}{}", self.order_hash),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef {
                    name: "one_inch_fusion_plus".into(),
                    chain: Some(src_chain),
                },
                sell: sell.clone(),
                buy,
                sell_max: self.maker_amount,
                buy_min: self.min_taker_amount,
                // Fusion+ is a Dutch-auction cross-chain swap.
                order_kind: OrderKind::Dutch,
            },
            commitment: AssetCommitment::PermitCap {
                token: sell,
                spender: Address::ZERO,
                max_out: self.maker_amount,
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status,
                valid_until: expiry,
                nonce: None, // orderHash lives in `id`
                on_chain_tx: None,
                raw_status,
            },
            sync: DataSource::VenueApi {
                endpoint: FUSION_PLUS_API_BASE.into(),
                parser_id: "one_inch_fusion_plus_orders".into(),
                auth: None,
            },
            signed_at: now,
            signature_payload: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Realistic by-maker response: a live cross-chain pending order (sell on
    // Ethereum, buy on Arbitrum), a settled (executed) order, and a Solana-leg
    // order that must be skipped.
    const ORDERS: &str = r#"{
      "meta": { "totalItems": 3, "currentPage": 1, "totalPages": 1, "itemsPerPage": 100 },
      "items": [
        {
          "orderHash": "0xcc01",
          "status": "pending",
          "validation": "valid",
          "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
          "takerAsset": "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
          "makerAmount": "1000000000",
          "minTakerAmount": "990000000",
          "srcChainId": 1,
          "dstChainId": 42161,
          "fills": [],
          "auctionStartDate": 1738000000,
          "auctionDuration": 180,
          "createdAt": "2025-01-27T00:00:00.000Z",
          "cancelTx": null
        },
        {
          "orderHash": "0xcc02",
          "status": "executed",
          "validation": "valid",
          "makerAsset": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
          "takerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
          "makerAmount": "2000000000000000000",
          "minTakerAmount": "5000000000",
          "srcChainId": 1,
          "dstChainId": 8453,
          "fills": [{ "status": "executed", "filledMakerAmount": "2000000000000000000" }],
          "auctionStartDate": 1738000000,
          "auctionDuration": 180,
          "createdAt": "2025-01-27T00:00:00.000Z"
        },
        {
          "orderHash": "0xcc03-solana",
          "status": "pending",
          "validation": "valid",
          "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
          "takerAsset": "0x0000000000000000000000000000000000000000",
          "makerAmount": "1000000000",
          "minTakerAmount": "1",
          "srcChainId": 1,
          "dstChainId": 501,
          "auctionStartDate": 1738000000,
          "auctionDuration": 180
        }
      ]
    }"#;

    #[test]
    fn status_mapping_covers_all_enum_values() {
        for (raw, want) in [
            ("pending", PendingStatus::Active),
            ("unpublished", PendingStatus::Active),
            ("executed", PendingStatus::Filled),
            ("expired", PendingStatus::Expired),
            ("cancelled", PendingStatus::Cancelled),
            ("refunding", PendingStatus::Failed),
            ("refunded", PendingStatus::Failed),
            ("something-new", PendingStatus::Unknown),
        ] {
            assert_eq!(
                map_one_inch_fusion_plus_status(raw, Some("valid")).0,
                want,
                "status {raw}"
            );
        }
        // validation is recorded in raw_status but does not flip a pending order.
        let (status, raw) = map_one_inch_fusion_plus_status("pending", Some("not-enough-balance"));
        assert_eq!(status, PendingStatus::Active);
        assert_eq!(raw.as_deref(), Some("pending/not-enough-balance"));
        // no validation present.
        assert_eq!(
            map_one_inch_fusion_plus_status("pending", None)
                .1
                .as_deref(),
            Some("pending")
        );
    }

    #[test]
    fn evm_chain_conversion_skips_solana() {
        assert_eq!(evm_chain_from_numeric(1), Some(ChainId::new("eip155:1")));
        assert_eq!(
            evm_chain_from_numeric(42161),
            Some(ChainId::new("eip155:42161"))
        );
        assert_eq!(evm_chain_from_numeric(SOLANA_NETWORK_ID), None);
    }

    #[test]
    fn cross_chain_order_puts_sell_on_src_and_buy_on_dst() {
        let value: serde_json::Value = serde_json::from_str(ORDERS).unwrap();
        let orders = parse_orders(&value).unwrap();
        assert_eq!(
            orders.len(),
            3,
            "all three parse; Solana skipped at projection"
        );

        let now = Time::from_unix(1_738_000_050);
        let p = orders[0].to_pending_tx(now).unwrap();
        assert_eq!(p.id, "intent:one_inch_fusion_plus:0xcc01");
        assert_eq!(p.lifecycle.status, PendingStatus::Active);
        assert_eq!(
            p.lifecycle.valid_until,
            Some(Time::from_unix(1_738_000_180))
        );
        match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue,
                sell,
                buy,
                order_kind,
                ..
            } => {
                assert_eq!(venue.name, "one_inch_fusion_plus");
                assert_eq!(venue.chain, Some(ChainId::new("eip155:1")));
                assert_eq!(*order_kind, OrderKind::Dutch);
                // sell on src (eth/1), buy on dst (arb/42161) — cross-chain.
                assert_eq!(sell.key.chain(), &ChainId::new("eip155:1"));
                assert_eq!(buy.key.chain(), &ChainId::new("eip155:42161"));
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }

        // executed → Filled (terminal; pruned by upsert).
        let p2 = orders[1].to_pending_tx(now).unwrap();
        assert_eq!(p2.lifecycle.status, PendingStatus::Filled);
        assert_eq!(p2.id, "intent:one_inch_fusion_plus:0xcc02");

        // Solana destination leg → skipped (cannot model as eip155).
        assert!(orders[2].to_pending_tx(now).is_none());
    }

    // --- producer-side tests: the real fetcher against a stub HTTP server,
    // proving fetch_orders is fail-closed (never a truncated/empty Ok). ---

    async fn spawn_seq_server(responses: Vec<serde_json::Value>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for body in responses {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 1024];
                loop {
                    let n = stream.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let s = body.to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    s.len(),
                    s
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });
        format!("http://{addr}")
    }

    fn fetcher(base_url: String) -> OneInchFusionPlusFetcher {
        OneInchFusionPlusFetcher::from_sync_config(&OneInchFusionPlusConfig {
            base_url,
            api_key: String::new(),
        })
    }

    fn order_item(hash: &str) -> serde_json::Value {
        serde_json::json!({
            "orderHash": hash,
            "status": "pending",
            "validation": "valid",
            "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "takerAsset": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "makerAmount": "1000",
            "minTakerAmount": "1",
            "srcChainId": 1,
            "dstChainId": 1
        })
    }

    #[tokio::test]
    async fn malformed_200_body_is_err_not_empty_ok() {
        // An unrecognized 200 body must be Err — NOT a silent empty Ok that would
        // snapshot-prune every live order.
        let base = spawn_seq_server(vec![serde_json::json!({ "unexpected": true })]).await;
        let r = fetcher(base)
            .fetch_orders(&Address::ZERO, Time::from_unix(0))
            .await;
        assert!(r.is_err(), "unrecognized 200 body must be Err, got {r:?}");
    }

    #[tokio::test]
    async fn full_page_with_one_undecodable_item_does_not_truncate_walk() {
        // page 1: 99 valid + 1 undecodable (missing orderHash) ⇒ raw_len 100,
        // parsed 99. The walk MUST continue to page 2 (terminator is the RAW
        // count, not 99 < 100).
        let mut items: Vec<serde_json::Value> =
            (0..99).map(|i| order_item(&format!("0xp1-{i}"))).collect();
        items.push(serde_json::json!({ "status": "pending", "srcChainId": 1, "dstChainId": 1 }));
        let page1 = serde_json::json!({ "items": items });
        let page2 = serde_json::json!({ "items": [order_item("0xp2-0")] });

        let base = spawn_seq_server(vec![page1, page2]).await;
        let out = fetcher(base)
            .fetch_orders(&Address::ZERO, Time::from_unix(0))
            .await
            .unwrap();

        assert_eq!(
            out.len(),
            100,
            "99 from page 1 + 1 from page 2 (page 2 was fetched)"
        );
        assert!(out
            .iter()
            .any(|p| p.id == "intent:one_inch_fusion_plus:0xp2-0"));
    }
}
