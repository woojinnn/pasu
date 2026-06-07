//! 1inch Fusion intent-order fetcher (same-chain Fusion, **not** Fusion+).
//!
//! Discovery uses the 1inch Dev Portal Fusion API, which **requires** an
//! `Authorization: Bearer <key>` header:
//! `GET https://api.1inch.dev/fusion/v2.0/{chainId}/order/maker/{address}/?page=1&limit=100`.
//! The chainId is the integer from the configured `eip155:<n>` chain, so each
//! configured chain is polled separately and paginated by `page`.
//!
//! Field names verified against the `@1inch/fusion-sdk` `OrderFillsByMakerOutput`
//! type (`orderHash` / `status` / `makerAsset` / `takerAsset` / `makerAmount` /
//! `minTakerAmount` / `fills` / `auctionStartDate` / `auctionDuration` /
//! `createdAt`) and the `OrderStatus` enum (`pending` / `filled` /
//! `false-predicate` / `not-enough-balance-or-allowance` / `expired` /
//! `partially-filled` / `wrong-permit` / `cancelled` / `invalid-signature`).
//!
//! The list-response envelope key (`items` vs a bare array) is decoded
//! defensively — see [`parse_orders`].

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;

use policy_state::pending::{
    AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::{DataSource, StateDelta};

use crate::config::OneInchFusionConfig;
use crate::error::SyncError;
use crate::fetchers::venue::IntentFetcher;

/// Default Dev Portal Fusion base. The `{chainId}` segment is in the path.
pub const FUSION_API_BASE: &str = "https://api.1inch.dev/fusion";

/// Page size per request.
const PAGE_LIMIT: usize = 100;

/// Fetches 1inch Fusion order status from the Dev Portal Fusion API.
pub struct OneInchFusionFetcher {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    chains: Vec<ChainId>,
}

impl OneInchFusionFetcher {
    #[must_use]
    pub fn from_sync_config(cfg: &OneInchFusionConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            base_url: cfg.base_url.clone(),
            api_key: cfg.api_key.clone(),
            chains: cfg.chains.clone(),
        }
    }

    /// Poll one chain: paginate `GET {base}/v2.0/{chainId}/order/maker/{address}/`
    /// by `page` until a short page is returned, projecting each order into a
    /// `PendingTx`.
    async fn fetch_chain(
        &self,
        chain: &ChainId,
        swapper: &Address,
        now: Time,
    ) -> Result<Vec<PendingTx>, SyncError> {
        let Some(chain_id) = eip155_numeric(chain) else {
            return Err(SyncError::FetchFailed {
                source_id: "one_inch_fusion".into(),
                reason: format!("non-eip155 chain {}", chain.as_str()),
            });
        };
        let mut out = Vec::new();
        let mut page = 1usize;
        loop {
            let url = format!(
                "{}/v2.0/{chain_id}/order/maker/{:#x}/?page={page}&limit={PAGE_LIMIT}",
                self.base_url.trim_end_matches('/'),
                swapper,
            );
            let body = self.get(&url).await?;
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
            page += 1;
        }
        Ok(out)
    }

    async fn get(&self, url: &str) -> Result<serde_json::Value, SyncError> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "one_inch_fusion".into(),
                reason: format!("http: {e}"),
            })?;
        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "one_inch_fusion".into(),
                reason: format!("status {}", resp.status()),
            });
        }
        resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "one_inch_fusion".into(),
            reason: format!("decode: {e}"),
        })
    }
}

#[async_trait]
impl IntentFetcher for OneInchFusionFetcher {
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
        if out.is_empty() {
            if let Some(e) = last_err {
                return Err(e);
            }
        }
        Ok(out)
    }
}

/// `eip155:<n>` → `<n>`. `None` for non-EVM chains.
#[must_use]
pub fn eip155_numeric(chain: &ChainId) -> Option<u64> {
    chain.as_str().strip_prefix("eip155:")?.parse().ok()
}

/// Map a Fusion `status` string to our canonical `PendingStatus`.
///
/// The `partially-filled` value is ambiguous (still live while the auction is
/// open, terminal after expiry), so it is disambiguated by `now < expiry`. The
/// second tuple element is the verbatim venue string, stored in
/// `PendingLifecycle.raw_status`.
#[must_use]
pub fn map_one_inch_fusion_status(
    raw: &str,
    now: Time,
    expiry: Option<Time>,
) -> (PendingStatus, Option<String>) {
    let live = expiry.is_none_or(|e| now < e);
    let status = match raw {
        "pending" => PendingStatus::Active,
        "partially-filled" => {
            if live {
                PendingStatus::PartiallyFilled
            } else {
                PendingStatus::Filled
            }
        }
        "filled" => PendingStatus::Filled,
        "expired" => PendingStatus::Expired,
        "cancelled" => PendingStatus::Cancelled,
        "false-predicate"
        | "not-enough-balance-or-allowance"
        | "wrong-permit"
        | "invalid-signature" => PendingStatus::Failed,
        _ => PendingStatus::Unknown,
    };
    (status, Some(raw.to_owned()))
}

/// One Fusion order (`OrderFillsByMakerOutput`). Only the projected fields are
/// decoded; unknown fields are ignored (defensive serde).
#[derive(Debug, Clone)]
pub struct OneInchFusionOrder {
    pub order_hash: String,
    pub status: String,
    pub maker_asset: Address,
    pub taker_asset: Address,
    pub maker_amount: U256,
    pub min_taker_amount: U256,
    pub auction_start: Option<u64>,
    pub auction_duration: Option<u64>,
}

/// Decode the Fusion list response.
///
/// The Dev Portal returns either a bare array or
/// `{ "items": [...], "meta": {...} }`; both are tolerated. Elements lacking a
/// decodable `orderHash` / `status` / `makerAsset` / `takerAsset` are skipped.
#[must_use]
pub fn parse_orders(value: &serde_json::Value) -> Option<Vec<OneInchFusionOrder>> {
    let arr = value
        .as_array()
        .or_else(|| value.get("items").and_then(serde_json::Value::as_array))?;
    Some(arr.iter().filter_map(parse_one).collect())
}

fn parse_one(o: &serde_json::Value) -> Option<OneInchFusionOrder> {
    let order_hash = o.get("orderHash")?.as_str()?.to_owned();
    let status = o.get("status")?.as_str()?.to_owned();
    let maker_asset = Address::from_str(o.get("makerAsset")?.as_str()?).ok()?;
    let taker_asset = Address::from_str(o.get("takerAsset")?.as_str()?).ok()?;
    let maker_amount = amount(o, "makerAmount");
    let min_taker_amount = amount(o, "minTakerAmount");
    // `auctionStartDate` / `auctionDuration` are unix-seconds integers but the
    // API sometimes serializes large numbers as strings — tolerate both.
    let auction_start = u64_field(o, "auctionStartDate");
    let auction_duration = u64_field(o, "auctionDuration");
    Some(OneInchFusionOrder {
        order_hash,
        status,
        maker_asset,
        taker_asset,
        maker_amount,
        min_taker_amount,
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

impl OneInchFusionOrder {
    /// `auctionStartDate + auctionDuration` as the order expiry, when both are
    /// present.
    #[must_use]
    pub fn expiry(&self) -> Option<Time> {
        match (self.auction_start, self.auction_duration) {
            (Some(s), Some(d)) => Some(Time::from_unix(s.saturating_add(d))),
            _ => None,
        }
    }

    /// Project this order into a `PendingTx`. The spender is `Address::ZERO`
    /// (Fusion resolves the filler at settlement time — matches `project_venue`).
    #[must_use]
    pub fn to_pending_tx(&self, chain: &ChainId, now: Time) -> PendingTx {
        let expiry = self.expiry();
        let (status, raw_status) = map_one_inch_fusion_status(&self.status, now, expiry);
        let sell = token_ref(chain, self.maker_asset);
        let buy = token_ref(chain, self.taker_asset);

        PendingTx {
            id: format!("intent:one_inch_fusion:{}", self.order_hash),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef {
                    name: "one_inch_fusion".into(),
                    chain: Some(chain.clone()),
                },
                sell: sell.clone(),
                buy,
                sell_max: self.maker_amount,
                buy_min: self.min_taker_amount,
                // Fusion is a Dutch-auction RFQ flow.
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
                endpoint: FUSION_API_BASE.into(),
                parser_id: "one_inch_fusion_orders".into(),
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

    // Realistic Fusion list response (the `items` envelope form). One pending
    // order with a live auction, one partially-filled.
    const ORDERS: &str = r#"{
      "meta": { "totalItems": 2, "currentPage": 1, "totalPages": 1 },
      "items": [
        {
          "orderHash": "0xfeed01",
          "status": "pending",
          "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
          "takerAsset": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
          "makerAmount": "1000000000",
          "minTakerAmount": "300000000000000000",
          "fills": [],
          "auctionStartDate": 1738000000,
          "auctionDuration": 180,
          "createdAt": "2025-01-27T00:00:00.000Z"
        },
        {
          "orderHash": "0xfeed02",
          "status": "partially-filled",
          "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
          "takerAsset": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
          "makerAmount": "500000000",
          "minTakerAmount": "100000000000000000",
          "fills": [{ "txHash": "0xabc", "filledMakerAmount": "100000000" }],
          "auctionStartDate": 1738000000,
          "auctionDuration": 180,
          "createdAt": "2025-01-27T00:00:00.000Z"
        }
      ]
    }"#;

    #[test]
    fn status_mapping_covers_all_enum_values_and_partial_fill_edge() {
        let start = Time::from_unix(1_000);
        let expiry = Some(Time::from_unix(1_180));
        let during = Time::from_unix(1_100);
        let after = Time::from_unix(2_000);

        assert_eq!(
            map_one_inch_fusion_status("pending", during, expiry).0,
            PendingStatus::Active
        );
        // partially-filled while auction live → PartiallyFilled ...
        assert_eq!(
            map_one_inch_fusion_status("partially-filled", during, expiry).0,
            PendingStatus::PartiallyFilled
        );
        // ... and terminal (Filled) after expiry.
        assert_eq!(
            map_one_inch_fusion_status("partially-filled", after, expiry).0,
            PendingStatus::Filled
        );
        // partially-filled with no expiry → treated as live.
        assert_eq!(
            map_one_inch_fusion_status("partially-filled", during, None).0,
            PendingStatus::PartiallyFilled
        );
        assert_eq!(
            map_one_inch_fusion_status("filled", after, expiry).0,
            PendingStatus::Filled
        );
        assert_eq!(
            map_one_inch_fusion_status("expired", after, expiry).0,
            PendingStatus::Expired
        );
        assert_eq!(
            map_one_inch_fusion_status("cancelled", during, expiry).0,
            PendingStatus::Cancelled
        );
        for failed in [
            "false-predicate",
            "not-enough-balance-or-allowance",
            "wrong-permit",
            "invalid-signature",
        ] {
            assert_eq!(
                map_one_inch_fusion_status(failed, during, expiry).0,
                PendingStatus::Failed,
                "status for {failed}"
            );
        }
        assert_eq!(
            map_one_inch_fusion_status("something-new", during, expiry).0,
            PendingStatus::Unknown
        );
        // raw preserved.
        assert_eq!(
            map_one_inch_fusion_status("pending", during, expiry)
                .1
                .as_deref(),
            Some("pending")
        );
        let _ = start;
    }

    #[test]
    fn parses_order_into_pending_tx() {
        let value: serde_json::Value = serde_json::from_str(ORDERS).unwrap();
        let orders = parse_orders(&value).unwrap();
        assert_eq!(orders.len(), 2);

        // `now` before expiry (start 1738000000 + 180 = 1738000180).
        let now = Time::from_unix(1_738_000_050);
        let p = orders[0].to_pending_tx(&ChainId::ethereum_mainnet(), now);
        assert_eq!(p.id, "intent:one_inch_fusion:0xfeed01");
        assert_eq!(p.lifecycle.status, PendingStatus::Active);
        // expiry = auctionStartDate + auctionDuration.
        assert_eq!(
            p.lifecycle.valid_until,
            Some(Time::from_unix(1_738_000_180))
        );
        match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue,
                sell_max,
                buy_min,
                order_kind,
                ..
            } => {
                assert_eq!(venue.name, "one_inch_fusion");
                assert_eq!(venue.chain, Some(ChainId::ethereum_mainnet()));
                assert_eq!(*order_kind, OrderKind::Dutch);
                assert_eq!(*sell_max, U256::from(1_000_000_000u64));
                assert_eq!(*buy_min, U256::from(300_000_000_000_000_000u64));
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
        // Fusion spender placeholder is the zero address.
        match &p.commitment {
            AssetCommitment::PermitCap { spender, .. } => assert_eq!(*spender, Address::ZERO),
            other => panic!("expected PermitCap, got {other:?}"),
        }

        // Second: partially-filled while auction live → PartiallyFilled.
        let p2 = orders[1].to_pending_tx(&ChainId::ethereum_mainnet(), now);
        assert_eq!(p2.lifecycle.status, PendingStatus::PartiallyFilled);

        // Same order after expiry → Filled (terminal).
        let p2_late =
            orders[1].to_pending_tx(&ChainId::ethereum_mainnet(), Time::from_unix(1_738_999_999));
        assert_eq!(p2_late.lifecycle.status, PendingStatus::Filled);
    }

    #[test]
    fn eip155_numeric_extracts_chain_id() {
        assert_eq!(eip155_numeric(&ChainId::ethereum_mainnet()), Some(1));
        assert_eq!(eip155_numeric(&ChainId::arbitrum()), Some(42161));
        assert_eq!(eip155_numeric(&ChainId::new("solana:foo")), None);
    }
}
