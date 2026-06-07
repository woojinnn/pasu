//! 1inch Limit Order Protocol (LOP) v4 intent-order fetcher.
//!
//! Discovery uses the 1inch Dev Portal Orderbook API (v4.1), which **requires**
//! an `Authorization: Bearer <key>` header (the same Dev Portal key as Fusion):
//! `GET https://api.1inch.dev/orderbook/v4.1/{chainId}/address/{maker}?limit=100&cursor=…`.
//! The chainId is a path segment, so each configured chain is polled separately,
//! cursor-paginated.
//!
//! **Active-orderbook semantics (load-bearing).** This endpoint returns only a
//! maker's currently-OPEN orders; a filled / cancelled / expired order silently
//! drops off the list rather than appearing with a terminal status. So
//! `upsert_intent_orders` alone could never retire a vanished order. This fetcher
//! therefore returns an `authoritative_prefix` (`intent:one_inch_limit_order:`),
//! and `sync_intent_orders` snapshot-prunes any tracked LOP order absent from a
//! successful fetch (it left the book → no longer open). We do NOT fabricate a
//! `Filled` vs `Cancelled` verdict for vanished orders; the cases we *can*
//! classify before they vanish are caught directly: `remainingMakerAmount == 0`
//! → `Filled`, and a past `makerTraits` expiry → `Expired`.
//!
//! **Prune safety.** Because the snapshot-prune trusts the returned set as
//! complete, `fetch_orders` returns `Err` on ANY incompleteness — never a partial
//! `Ok`. This covers more than transport errors: `?` handles a failed chain /
//! non-2xx / decode error, and `fetch_chain` additionally fails closed on a 200
//! whose body is an unrecognized shape, on `meta.hasMore` with no usable cursor,
//! and on `MAX_PAGES` exhaustion. A non-empty book is only ever treated as
//! complete after an explicit `hasMore == false`.
//!
//! Field names + the `makerTraits` expiration bit layout are verified against
//! `@1inch/limit-order-sdk` (`LimitOrderApiItem` in `src/api/types.ts`;
//! `EXPIRATION = BitMask(80,120)` in `src/limit-order/maker-traits.ts`). The LOP
//! v4 contract (EIP-712 verifyingContract == the maker-funds spender) is the
//! 1inch `AggregationRouterV6`, the same deterministic address on every EVM chain
//! except zkSync Era.

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;

use policy_state::pending::{
    AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::{DataSource, StateDelta};

use crate::config::OneInchLopConfig;
use crate::error::SyncError;
use crate::fetchers::venue::one_inch_fusion::eip155_numeric;
use crate::fetchers::venue::IntentFetcher;

/// Default Dev Portal Orderbook base. The `/v4.1/{chainId}/...` segments follow.
pub const LOP_API_BASE: &str = "https://api.1inch.dev/orderbook";

/// `PendingTx` id prefix for this venue. Also the `authoritative_prefix`. Matches
/// `IntentVenue::OneInchLimitOrder`'s `name()` so state-side orders line up with
/// the engine's venue identity.
pub const LOP_PREFIX: &str = "intent:one_inch_limit_order:";

/// 1inch `AggregationRouterV6` (= LOP v4 verifyingContract == maker-funds spender),
/// the deterministic address on every EVM chain except zkSync Era. Verified
/// against the 1inch SDK + Etherscan "1inch: Aggregation Router V6" label.
const LOP_V4_ROUTER: &str = "0x111111125421ca6dc452d289314280a0f8842a65";
/// zkSync Era (chain id 324) uses a distinct deterministic address.
const LOP_V4_ROUTER_ZKSYNC: &str = "0x6fd4383cb451173d5f9304f041c7bcbf27d561ff";
const ZKSYNC_CHAIN_ID: u64 = 324;

/// `makerTraits` expiration occupies bits [80, 120) (uint40, unix seconds).
const EXPIRATION_OFFSET: usize = 80;

/// Page size per request.
const PAGE_LIMIT: usize = 100;

/// Pagination safety bound (never expect this many pages for one maker).
const MAX_PAGES: usize = 1000;

/// Fetches 1inch LOP v4 open-order status from the Dev Portal Orderbook API.
pub struct OneInchLopFetcher {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    chains: Vec<ChainId>,
}

impl OneInchLopFetcher {
    #[must_use]
    pub fn from_sync_config(cfg: &OneInchLopConfig) -> Self {
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

    /// Poll one chain: cursor-paginate `GET {base}/v4.1/{chainId}/address/{maker}`
    /// until `meta.hasMore` is false.
    ///
    /// **Fail-closed for snapshot-prune safety.** Returns `Err` on ANY
    /// incompleteness so a partial walk never reaches the snapshot-prune: a
    /// transport/non-2xx/decode error (via `?` in `get`); a 200 whose body is an
    /// unrecognized shape (`parse_orders` → `None`, e.g. a 200-wrapped error
    /// envelope or schema drift — `?`-propagation does NOT cover this, so it is
    /// handled explicitly); `meta.hasMore == true` with no usable `nextCursor`;
    /// or exhausting `MAX_PAGES` while more pages remain. Only an explicit
    /// `hasMore == false` (or no pagination envelope) is a clean completion.
    async fn fetch_chain(
        &self,
        chain: &ChainId,
        chain_id: u64,
        swapper: &Address,
        now: Time,
        out: &mut Vec<PendingTx>,
    ) -> Result<(), SyncError> {
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut url = format!(
                "{}/v4.1/{chain_id}/address/{:#x}?limit={PAGE_LIMIT}",
                self.base_url.trim_end_matches('/'),
                swapper,
            );
            if let Some(c) = &cursor {
                url.push_str("&cursor=");
                url.push_str(c);
            }
            let body = self.get(&url).await?;
            let orders = parse_orders(&body).ok_or_else(|| SyncError::FetchFailed {
                source_id: "one_inch_lop".into(),
                reason: "unrecognized orderbook response shape (not an array or {items:[...]})"
                    .into(),
            })?;
            for o in &orders {
                out.push(o.to_pending_tx(chain, chain_id, now));
            }
            match page_continuation(&body)? {
                Some(c) => cursor = Some(c),
                None => return Ok(()),
            }
        }
        Err(SyncError::FetchFailed {
            source_id: "one_inch_lop".into(),
            reason: format!("orderbook walk exceeded {MAX_PAGES} pages; treating as incomplete"),
        })
    }

    async fn get(&self, url: &str) -> Result<serde_json::Value, SyncError> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "one_inch_lop".into(),
                reason: format!("http: {e}"),
            })?;
        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "one_inch_lop".into(),
                reason: format!("status {}", resp.status()),
            });
        }
        resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "one_inch_lop".into(),
            reason: format!("decode: {e}"),
        })
    }
}

#[async_trait]
impl IntentFetcher for OneInchLopFetcher {
    async fn fetch_orders(
        &self,
        swapper: &Address,
        now: Time,
    ) -> Result<Vec<PendingTx>, SyncError> {
        let mut out = Vec::new();
        for chain in &self.chains {
            // LOP is EVM-only; a non-eip155 configured chain has no orders, so
            // skipping it keeps the returned set complete for the prefix.
            let Some(chain_id) = eip155_numeric(chain) else {
                continue;
            };
            // `?` — any chain's failure aborts with Err (no partial Ok), so the
            // snapshot-prune never drops live orders on a transient error.
            self.fetch_chain(chain, chain_id, swapper, now, &mut out)
                .await?;
        }
        Ok(out)
    }

    fn authoritative_prefix(&self) -> Option<&str> {
        Some(LOP_PREFIX)
    }
}

/// The maker-funds spender (= LOP v4 verifyingContract) for `chain_id`.
#[must_use]
pub fn lop_spender(chain_id: u64) -> Address {
    let s = if chain_id == ZKSYNC_CHAIN_ID {
        LOP_V4_ROUTER_ZKSYNC
    } else {
        LOP_V4_ROUTER
    };
    Address::from_str(s).unwrap_or(Address::ZERO)
}

/// Decode the order expiration from the `makerTraits` uint256: bits [80, 120)
/// (uint40, unix seconds). `0` means no expiry → `None`.
#[must_use]
pub fn decode_maker_traits_expiry(maker_traits: U256) -> Option<Time> {
    // 40-bit mask = 2^40 - 1.
    let mask = U256::from(0xFF_FFFF_FFFFu64);
    let exp = (maker_traits >> EXPIRATION_OFFSET) & mask;
    if exp == U256::ZERO {
        None
    } else {
        Some(Time::from_unix(exp.to::<u64>()))
    }
}

/// Derive lifecycle status from fill progress and expiry.
///
/// The active orderbook only lists open orders, so terminal Cancelled is detected
/// via snapshot-prune, not here; this catches the two cases observable while still
/// listed.
#[must_use]
pub fn lop_status(remaining: U256, making: U256, expiry: Option<Time>, now: Time) -> PendingStatus {
    if let Some(e) = expiry {
        if now >= e {
            return PendingStatus::Expired;
        }
    }
    if remaining == U256::ZERO {
        PendingStatus::Filled
    } else if remaining < making {
        PendingStatus::PartiallyFilled
    } else {
        PendingStatus::Active
    }
}

/// One LOP v4 order (`LimitOrderApiItem`). Only projected fields decoded.
#[derive(Debug, Clone)]
pub struct OneInchLopOrder {
    pub order_hash: String,
    pub maker_asset: Address,
    pub taker_asset: Address,
    pub making_amount: U256,
    pub taking_amount: U256,
    pub remaining_maker_amount: U256,
    pub maker_traits: U256,
    /// Joined `orderInvalidReason` (null when the order is valid).
    pub invalid_reason: Option<String>,
}

/// Decode the orderbook list response (`{ items: [...], meta: {...} }` or a bare
/// array). Elements lacking decodable required fields are skipped.
#[must_use]
pub fn parse_orders(value: &serde_json::Value) -> Option<Vec<OneInchLopOrder>> {
    let arr = value
        .as_array()
        .or_else(|| value.get("items").and_then(serde_json::Value::as_array))?;
    Some(arr.iter().filter_map(parse_one).collect())
}

fn parse_one(o: &serde_json::Value) -> Option<OneInchLopOrder> {
    let order_hash = o.get("orderHash")?.as_str()?.to_owned();
    let data = o.get("data")?;
    let maker_asset = Address::from_str(data.get("makerAsset")?.as_str()?).ok()?;
    let taker_asset = Address::from_str(data.get("takerAsset")?.as_str()?).ok()?;
    let making_amount = u256_field(data, "makingAmount")?;
    let taking_amount = u256_field(data, "takingAmount").unwrap_or(U256::ZERO);
    let maker_traits = u256_field(data, "makerTraits").unwrap_or(U256::ZERO);
    // Absent remaining ⇒ treat as untouched (== making) rather than filled.
    let remaining_maker_amount = u256_field(o, "remainingMakerAmount").unwrap_or(making_amount);
    let invalid_reason = parse_invalid_reason(o.get("orderInvalidReason"));
    Some(OneInchLopOrder {
        order_hash,
        maker_asset,
        taker_asset,
        making_amount,
        taking_amount,
        remaining_maker_amount,
        maker_traits,
        invalid_reason,
    })
}

/// `orderInvalidReason` is `null | string[]`; join non-empty into a single tag.
fn parse_invalid_reason(v: Option<&serde_json::Value>) -> Option<String> {
    let arr = v?.as_array()?;
    let joined = arr
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>()
        .join(",");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// Parse a uint256 that may be a decimal string, a `0x`-hex string, or a JSON
/// number (used for amounts AND the `makerTraits` bitfield).
fn u256_field(o: &serde_json::Value, key: &str) -> Option<U256> {
    let v = o.get(key)?;
    if let Some(s) = v.as_str() {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            return U256::from_str_radix(hex, 16).ok();
        }
        return U256::from_str_radix(s, 10).ok();
    }
    v.as_u64().map(U256::from)
}

/// Decide whether to continue paginating, fail-closed.
/// * `Ok(None)` — clean completion: no `meta` envelope (single-page / bare array)
///   or an explicit `meta.hasMore == false`.
/// * `Ok(Some(cursor))` — `hasMore == true` with a usable next cursor.
/// * `Err(_)` — `hasMore == true` but no usable `nextCursor`: the API asserts
///   more pages exist yet gives no way to fetch them, so the walk is incomplete
///   and must NOT be treated as a complete set by the snapshot-prune.
fn page_continuation(body: &serde_json::Value) -> Result<Option<String>, SyncError> {
    let Some(meta) = body.get("meta") else {
        return Ok(None);
    };
    if !meta
        .get("hasMore")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(None);
    }
    match meta.get("nextCursor").and_then(serde_json::Value::as_str) {
        Some(c) if !c.is_empty() => Ok(Some(c.to_owned())),
        _ => Err(SyncError::FetchFailed {
            source_id: "one_inch_lop".into(),
            reason: "meta.hasMore=true but no usable nextCursor; orderbook walk incomplete".into(),
        }),
    }
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

impl OneInchLopOrder {
    /// Project into a `PendingTx`. LOP is single-chain (sell + buy on `chain`).
    #[must_use]
    pub fn to_pending_tx(&self, chain: &ChainId, chain_id: u64, now: Time) -> PendingTx {
        let valid_until = decode_maker_traits_expiry(self.maker_traits);
        let status = lop_status(
            self.remaining_maker_amount,
            self.making_amount,
            valid_until,
            now,
        );
        let sell = token_ref(chain, self.maker_asset);
        let buy = token_ref(chain, self.taker_asset);

        PendingTx {
            id: format!("{LOP_PREFIX}{}", self.order_hash),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef {
                    name: "one_inch_limit_order".into(),
                    chain: Some(chain.clone()),
                },
                sell: sell.clone(),
                buy,
                sell_max: self.making_amount,
                buy_min: self.taking_amount,
                order_kind: OrderKind::Limit,
            },
            commitment: AssetCommitment::PermitCap {
                token: sell,
                spender: lop_spender(chain_id),
                max_out: self.making_amount,
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status,
                valid_until,
                nonce: None, // orderHash lives in `id`
                on_chain_tx: None,
                raw_status: self.invalid_reason.clone(),
            },
            sync: DataSource::VenueApi {
                endpoint: LOP_API_BASE.into(),
                parser_id: "one_inch_lop_orders".into(),
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

    fn u(s: &str) -> U256 {
        u256_field(&serde_json::json!({ "k": s }), "k").unwrap()
    }

    #[test]
    fn decodes_maker_traits_expiry_from_verified_vectors() {
        // Vectors verified from @1inch/limit-order-sdk maker-traits.ts:
        // EXPIRATION = BitMask(80,120) (uint40 seconds), 0 = no expiry.
        // 1) expiration only.
        assert_eq!(
            decode_maker_traits_expiry(u("0x6553f10000000000000000000000")),
            Some(Time::from_unix(1_700_000_000))
        );
        // 2) no-expiry.
        assert_eq!(decode_maker_traits_expiry(U256::ZERO), None);
        // 3) high flag bits set above the field must not leak into the decode.
        assert_eq!(
            decode_maker_traits_expiry(u(
                "0xc0000000000000000000000000000000000070dbd88000000000000000000000"
            )),
            Some(Time::from_unix(1_893_456_000))
        );
        // 4) boundary isolation — adjacent fields on both sides of [80,120).
        assert_eq!(
            decode_maker_traits_expiry(u("0xabcd006553f10000112233445566778899")),
            Some(Time::from_unix(1_700_000_000))
        );
    }

    #[test]
    fn spender_is_router_v6_with_zksync_exception() {
        assert_eq!(
            lop_spender(1),
            Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap()
        );
        assert_eq!(
            lop_spender(42161),
            Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap()
        );
        assert_eq!(
            lop_spender(324),
            Address::from_str("0x6fd4383cb451173d5f9304f041c7bcbf27d561ff").unwrap()
        );
    }

    #[test]
    fn status_from_fill_and_expiry() {
        let making = U256::from(1000u64);
        let expiry = Some(Time::from_unix(1_700_000_000));
        let live = Time::from_unix(1_699_999_000);
        let after = Time::from_unix(1_700_000_500);

        assert_eq!(
            lop_status(making, making, expiry, live),
            PendingStatus::Active
        );
        assert_eq!(
            lop_status(U256::from(400u64), making, expiry, live),
            PendingStatus::PartiallyFilled
        );
        assert_eq!(
            lop_status(U256::ZERO, making, expiry, live),
            PendingStatus::Filled
        );
        // expiry past dominates regardless of remaining.
        assert_eq!(
            lop_status(making, making, expiry, after),
            PendingStatus::Expired
        );
        // no expiry → never Expired.
        assert_eq!(
            lop_status(making, making, None, after),
            PendingStatus::Active
        );
    }

    // Realistic orderbook response: one untouched live order, one partially
    // filled, both with a future expiry in their makerTraits.
    const ORDERS: &str = r#"{
      "meta": { "hasMore": false },
      "items": [
        {
          "orderHash": "0xlop01",
          "remainingMakerAmount": "1000000000",
          "orderInvalidReason": null,
          "data": {
            "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "takerAsset": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "makingAmount": "1000000000",
            "takingAmount": "300000000000000000",
            "makerTraits": "0x6553f10000000000000000000000"
          }
        },
        {
          "orderHash": "0xlop02",
          "remainingMakerAmount": "400000000",
          "orderInvalidReason": ["insufficient-balance"],
          "data": {
            "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "takerAsset": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "makingAmount": "1000000000",
            "takingAmount": "300000000000000000",
            "makerTraits": "0x6553f10000000000000000000000"
          }
        }
      ]
    }"#;

    #[test]
    fn parses_orders_into_pending_tx() {
        let value: serde_json::Value = serde_json::from_str(ORDERS).unwrap();
        let orders = parse_orders(&value).unwrap();
        assert_eq!(orders.len(), 2);
        assert!(matches!(page_continuation(&value), Ok(None)));

        let chain = ChainId::new("eip155:1");
        let now = Time::from_unix(1_699_000_000); // before the 1_700_000_000 expiry

        let p = orders[0].to_pending_tx(&chain, 1, now);
        assert_eq!(p.id, "intent:one_inch_limit_order:0xlop01");
        assert_eq!(p.lifecycle.status, PendingStatus::Active);
        assert_eq!(
            p.lifecycle.valid_until,
            Some(Time::from_unix(1_700_000_000))
        );
        assert_eq!(p.lifecycle.raw_status, None);
        match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue,
                sell_max,
                buy_min,
                order_kind,
                ..
            } => {
                assert_eq!(venue.name, "one_inch_limit_order");
                assert_eq!(venue.chain, Some(ChainId::new("eip155:1")));
                assert_eq!(*order_kind, OrderKind::Limit);
                assert_eq!(*sell_max, U256::from(1_000_000_000u64));
                assert_eq!(*buy_min, U256::from(300_000_000_000_000_000u64));
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
        // spender is the LOP v4 router.
        match &p.commitment {
            AssetCommitment::PermitCap { spender, .. } => assert_eq!(
                *spender,
                Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap()
            ),
            other => panic!("expected PermitCap, got {other:?}"),
        }

        // Second order: partially filled, with an invalid-reason recorded.
        let p2 = orders[1].to_pending_tx(&chain, 1, now);
        assert_eq!(p2.lifecycle.status, PendingStatus::PartiallyFilled);
        assert_eq!(
            p2.lifecycle.raw_status.as_deref(),
            Some("insufficient-balance")
        );
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

    fn fetcher(base_url: String) -> OneInchLopFetcher {
        OneInchLopFetcher::from_sync_config(&OneInchLopConfig {
            base_url,
            api_key: String::new(),
            chains: vec![ChainId::new("eip155:1")],
        })
    }

    fn lop_item(hash: &str) -> serde_json::Value {
        serde_json::json!({
            "orderHash": hash,
            "remainingMakerAmount": "1000000000",
            "orderInvalidReason": null,
            "data": {
                "makerAsset": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
                "takerAsset": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
                "makingAmount": "1000000000",
                "takingAmount": "300000000000000000",
                "makerTraits": "0x0"
            }
        })
    }

    #[tokio::test]
    async fn malformed_200_body_is_err_not_empty_ok() {
        let base = spawn_seq_server(vec![serde_json::json!({ "unexpected": true })]).await;
        let r = fetcher(base)
            .fetch_orders(&Address::ZERO, Time::from_unix(0))
            .await;
        assert!(r.is_err(), "unrecognized 200 body must be Err, got {r:?}");
    }

    #[tokio::test]
    async fn has_more_without_cursor_is_err() {
        // The API asserts more pages but gives no cursor → incomplete → Err
        // (never a partial Ok that would snapshot-prune the un-fetched orders).
        let body = serde_json::json!({ "items": [lop_item("0xa")], "meta": { "hasMore": true } });
        let base = spawn_seq_server(vec![body]).await;
        let r = fetcher(base)
            .fetch_orders(&Address::ZERO, Time::from_unix(0))
            .await;
        assert!(
            r.is_err(),
            "hasMore=true with no nextCursor must be Err, got {r:?}"
        );
    }

    #[tokio::test]
    async fn clean_single_page_is_ok() {
        let body = serde_json::json!({ "items": [lop_item("0xa"), lop_item("0xb")], "meta": { "hasMore": false } });
        let base = spawn_seq_server(vec![body]).await;
        let out = fetcher(base)
            .fetch_orders(&Address::ZERO, Time::from_unix(0))
            .await
            .unwrap();
        assert_eq!(out.len(), 2);
        assert!(out
            .iter()
            .any(|p| p.id == "intent:one_inch_limit_order:0xa"));
    }
}
