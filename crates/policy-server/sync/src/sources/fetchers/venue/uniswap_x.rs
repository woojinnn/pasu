//! Uniswap order fetcher — `UniswapX` order lifecycle.
//!
//! Discovery uses the public v2 order service:
//! `GET https://api.uniswap.org/v2/orders?swapper=<addr>&limit=20`.
//! Quirks confirmed against the live API (the hosted `trade-api.gateway` v1
//! endpoint requires `orderId`/`orderIds`, so it can't list by `swapper`):
//! - `chainId` may NOT be combined with `swapper`; each order carries its own
//!   `chainId` instead.
//! - `limit` must be `<= 20`.
//! - no api-key required (the key is only needed for the v1 gateway).
//! - order shape varies by `type`: Dutch legs decay (`startAmount`/`endAmount`)
//!   and carry a `deadline`; Priority legs are single `amount` with no deadline.

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;

use policy_state::pending::{
    AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::{DataSource, StateDelta};

use crate::config::UniswapConfig;
use crate::error::SyncError;
use crate::fetchers::venue::IntentFetcher;

/// Default public v2 order-service base URL (no api-key required).
pub const UNISWAP_V2_ORDERS_BASE: &str = "https://api.uniswap.org/v2";

/// Fetches `UniswapX` order status from the Uniswap v2 order service.
pub struct UniswapXFetcher {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl UniswapXFetcher {
    #[must_use]
    pub fn from_sync_config(cfg: &UniswapConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            base_url: cfg.orders_endpoint.clone(),
            api_key: cfg.api_key.clone(),
        }
    }

    /// `GET {base}/orders?swapper={swapper}&limit=20`. Returns the swapper's
    /// most-recent orders across all chains (each carries its own `chainId`).
    /// `chainId` is intentionally omitted — the v2 service rejects it alongside
    /// `swapper`.
    pub async fn fetch_orders(&self, swapper: &Address) -> Result<Vec<UniswapXOrder>, SyncError> {
        let url = format!(
            "{}/orders?swapper={:#x}&limit=20",
            self.base_url.trim_end_matches('/'),
            swapper,
        );
        let mut req = self.client.get(&url);
        if !self.api_key.is_empty() {
            req = req.header("x-api-key", &self.api_key);
        }
        let resp = req.send().await.map_err(|e| SyncError::FetchFailed {
            source_id: "uniswap_x".into(),
            reason: format!("http: {e}"),
        })?;
        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "uniswap_x".into(),
                reason: format!("status {}", resp.status()),
            });
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "uniswap_x".into(),
            reason: format!("decode: {e}"),
        })?;
        Ok(parse_orders(&body).unwrap_or_default())
    }
}

/// `UniswapX` V2 reactor on Ethereum mainnet (the permit-cap spender). Per-chain
/// reactors can be threaded through config later (spec §12).
#[must_use]
pub fn uniswap_x_reactor() -> Address {
    Address::from_str("0x00000011f84b9aa48e5f8aa8b9897600006289be").unwrap_or(Address::ZERO)
}

#[async_trait]
impl IntentFetcher for UniswapXFetcher {
    /// Discover this swapper's `UniswapX` orders and project each into a
    /// `PendingTx`. The inherent `fetch_orders(swapper)` (1-arg) lists the raw
    /// orders; this trait method (3-arg) layers the projection on top so the
    /// orchestrator can dispatch over `&dyn IntentFetcher`.
    async fn fetch_orders(
        &self,
        swapper: &Address,
        now: Time,
    ) -> Result<Vec<PendingTx>, SyncError> {
        let reactor = uniswap_x_reactor();
        // 1-arg method-call resolves to the inherent `fetch_orders` (the trait
        // method takes 3 args), so this is not a recursive self-call.
        let raw = self.fetch_orders(swapper).await?;
        Ok(raw
            .iter()
            .map(|o| o.to_pending_tx(reactor, swapper, now))
            .collect())
    }
}

/// Map a Uniswap order `orderStatus` string to our canonical `PendingStatus`.
/// The second tuple element is the verbatim venue string, stored in
/// `PendingLifecycle.raw_status`.
#[must_use]
pub fn map_uniswapx_status(raw: &str) -> (PendingStatus, Option<String>) {
    let status = match raw {
        "open" | "unverified" => PendingStatus::Active,
        "filled" => PendingStatus::Filled,
        "cancelled" => PendingStatus::Cancelled,
        "expired" => PendingStatus::Expired,
        "error" | "insufficient-funds" => PendingStatus::Failed,
        _ => PendingStatus::Unknown,
    };
    (status, Some(raw.to_owned()))
}

/// One `UniswapX` order as returned by `GET /v2/orders`. Only the fields we
/// project into state are decoded; unknown fields are ignored.
#[derive(Debug, Clone)]
pub struct UniswapXOrder {
    pub order_hash: String,
    pub order_status: String,
    pub order_type: String,
    pub chain_id: u64,
    pub deadline: Option<u64>,
    pub sell_token: Address,
    pub sell_amount: U256,
    pub buy_token: Address,
    pub buy_min: U256,
}

/// Decode the `{ "orders": [...] }` envelope, skipping any element that lacks a
/// decodable `orderHash` / `chainId` / `input` / `outputs`.
#[must_use]
pub fn parse_orders(value: &serde_json::Value) -> Option<Vec<UniswapXOrder>> {
    let arr = value.get("orders")?.as_array()?;
    Some(arr.iter().filter_map(parse_one).collect())
}

fn parse_one(o: &serde_json::Value) -> Option<UniswapXOrder> {
    let order_hash = o.get("orderHash")?.as_str()?.to_owned();
    let order_status = o.get("orderStatus")?.as_str()?.to_owned();
    let order_type = o
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let chain_id = o.get("chainId").and_then(serde_json::Value::as_u64)?;
    let deadline = o.get("deadline").and_then(serde_json::Value::as_u64);

    let input = o.get("input")?;
    let sell_token = Address::from_str(input.get("token")?.as_str()?).ok()?;
    let sell_amount = amount_max(input);

    let outputs = o.get("outputs")?.as_array()?;
    let primary = outputs.first()?;
    let buy_token = Address::from_str(primary.get("token")?.as_str()?).ok()?;
    let buy_min = amount_min(primary);

    Some(UniswapXOrder {
        order_hash,
        order_status,
        order_type,
        chain_id,
        deadline,
        sell_token,
        sell_amount,
        buy_token,
        buy_min,
    })
}

/// Dutch decay endpoints `(startAmount, endAmount)` if present.
fn dutch_endpoints(leg: &serde_json::Value) -> Option<(U256, U256)> {
    let start = u256_dec(leg.get("startAmount")?.as_str()?);
    let end = u256_dec(leg.get("endAmount")?.as_str()?);
    Some((start, end))
}

/// Largest amount this leg could move: Priority single `amount`, else Dutch
/// `max(startAmount, endAmount)`.
fn amount_max(leg: &serde_json::Value) -> U256 {
    if let Some(a) = leg.get("amount").and_then(serde_json::Value::as_str) {
        return u256_dec(a);
    }
    dutch_endpoints(leg).map_or(U256::ZERO, |(s, e)| s.max(e))
}

/// Smallest amount this leg guarantees: Priority single `amount`, else Dutch
/// `min(startAmount, endAmount)` (the worst-case `endAmount`).
fn amount_min(leg: &serde_json::Value) -> U256 {
    if let Some(a) = leg.get("amount").and_then(serde_json::Value::as_str) {
        return u256_dec(a);
    }
    dutch_endpoints(leg).map_or(U256::ZERO, |(s, e)| s.min(e))
}

/// Decimal-string wei → `U256`; `0` on parse failure (amounts are venue-supplied).
fn u256_dec(s: &str) -> U256 {
    U256::from_str(s).unwrap_or(U256::ZERO)
}

fn order_kind_for(order_type: &str) -> OrderKind {
    match order_type {
        "Limit" | "LIMIT_ORDER" => OrderKind::Limit,
        "Priority" | "PRIORITY" => OrderKind::Rfq,
        _ => OrderKind::Dutch, // Dutch / Dutch_V2 / Dutch_V3 / unknown
    }
}

/// `<n>` → `ChainId("eip155:<n>")`.
fn chain_from_eip155(chain_id: u64) -> ChainId {
    ChainId(format!("eip155:{chain_id}"))
}

fn token_ref(chain: &ChainId, address: Address) -> TokenRef {
    // UniswapX uses the zero / 0xeee… sentinel for native ETH.
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

impl UniswapXOrder {
    /// Project this order into a `PendingTx`. `reactor` is the permit-cap
    /// spender. The chain comes from the order's own `chainId`; the venue
    /// `orderHash` is embedded in the id so upserts are idempotent and a future
    /// reducer-reconciliation can join on it.
    #[must_use]
    pub fn to_pending_tx(&self, reactor: Address, _swapper: &Address, now: Time) -> PendingTx {
        let (status, raw_status) = map_uniswapx_status(&self.order_status);
        let chain = chain_from_eip155(self.chain_id);
        let sell = token_ref(&chain, self.sell_token);
        let buy = token_ref(&chain, self.buy_token);

        PendingTx {
            id: format!("intent:uniswap_x:{}", self.order_hash),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef {
                    name: "uniswap_x".into(),
                    chain: Some(chain),
                },
                sell: sell.clone(),
                buy,
                sell_max: self.sell_amount,
                buy_min: self.buy_min,
                order_kind: order_kind_for(&self.order_type),
            },
            commitment: AssetCommitment::PermitCap {
                token: sell,
                spender: reactor,
                max_out: self.sell_amount,
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status,
                valid_until: self.deadline.map(Time::from_unix),
                nonce: None, // orderHash lives in `id`; typed nonce wiring deferred
                on_chain_tx: None,
                raw_status,
            },
            sync: DataSource::VenueApi {
                endpoint: UNISWAP_V2_ORDERS_BASE.into(),
                parser_id: "uniswap_x_orders".into(),
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

    // Dutch order: decaying input/output (`startAmount`/`endAmount`) + deadline.
    const DUTCH_ORDERS: &str = r#"{
      "orders": [
        {
          "orderHash": "0xabc123",
          "orderStatus": "open",
          "chainId": 1,
          "swapper": "0x000000000000000000000000000000000000a01c",
          "type": "Dutch_V2",
          "deadline": 1738003600,
          "input": { "token": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "startAmount": "600000000", "endAmount": "600000000" },
          "outputs": [
            { "token": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "startAmount": "300000000000000000", "endAmount": "295000000000000000", "recipient": "0x000000000000000000000000000000000000a01c" }
          ]
        }
      ]
    }"#;

    // Priority order: single `amount` legs, no `deadline`, native (zero) output, Base chain.
    const PRIORITY_ORDERS: &str = r#"{
      "orders": [
        {
          "orderHash": "0xdef456",
          "orderStatus": "filled",
          "chainId": 8453,
          "type": "Priority",
          "input": { "token": "0x1b6A569DD61EdCe3C383f6D565e2f79Ec3a12980", "amount": "3333341400000000000000000", "mpsPerPriorityFeeWei": "0" },
          "outputs": [
            { "token": "0x0000000000000000000000000000000000000000", "amount": "3167725861604571352", "recipient": "0xd8da6bf26964af9d7eed9e03e53415d37aa96045", "mpsPerPriorityFeeWei": "1" }
          ]
        }
      ]
    }"#;

    #[test]
    fn status_mapping_is_exhaustive_and_preserves_raw() {
        let cases = [
            ("open", PendingStatus::Active),
            ("unverified", PendingStatus::Active),
            ("filled", PendingStatus::Filled),
            ("cancelled", PendingStatus::Cancelled),
            ("expired", PendingStatus::Expired),
            ("error", PendingStatus::Failed),
            ("insufficient-funds", PendingStatus::Failed),
            ("something-new", PendingStatus::Unknown),
        ];
        for (raw, want) in cases {
            let (got, kept) = map_uniswapx_status(raw);
            assert_eq!(got, want, "status for {raw}");
            assert_eq!(kept.as_deref(), Some(raw), "raw preserved for {raw}");
        }
    }

    #[test]
    fn parses_dutch_order_into_pending_tx() {
        let swapper = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
        let reactor = Address::from_str("0x6000da47483062A0D734Ba3dc7576Ce6A0b645c4").unwrap();
        let value: serde_json::Value = serde_json::from_str(DUTCH_ORDERS).unwrap();

        let orders = parse_orders(&value).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].chain_id, 1);

        let p = orders[0].to_pending_tx(reactor, &swapper, Time::from_unix(1_738_000_000));
        assert_eq!(p.id, "intent:uniswap_x:0xabc123");
        assert_eq!(p.lifecycle.status, PendingStatus::Active);
        assert_eq!(
            p.lifecycle.valid_until,
            Some(Time::from_unix(1_738_003_600))
        );
        match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue,
                sell_max,
                buy_min,
                order_kind,
                ..
            } => {
                assert_eq!(venue.chain, Some(ChainId::ethereum_mainnet()));
                assert_eq!(*order_kind, OrderKind::Dutch);
                assert_eq!(*sell_max, U256::from(600_000_000u64)); // input amount
                assert_eq!(*buy_min, U256::from(295_000_000_000_000_000u64)); // output endAmount
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
    }

    #[test]
    fn parses_priority_order_amount_legs_and_no_deadline() {
        let swapper = Address::ZERO;
        let value: serde_json::Value = serde_json::from_str(PRIORITY_ORDERS).unwrap();

        let orders = parse_orders(&value).unwrap();
        assert_eq!(orders.len(), 1);
        let o = &orders[0];
        assert_eq!(o.chain_id, 8453);
        assert_eq!(o.deadline, None);
        // Priority legs use a single `amount`.
        assert_eq!(
            o.sell_amount,
            U256::from_str("3333341400000000000000000").unwrap()
        );
        assert_eq!(o.buy_min, U256::from_str("3167725861604571352").unwrap());

        let p = o.to_pending_tx(Address::ZERO, &swapper, Time::from_unix(0));
        assert_eq!(p.lifecycle.status, PendingStatus::Filled);
        assert_eq!(p.lifecycle.valid_until, None); // no deadline on Priority
        match &p.kind {
            PendingKind::OffchainLimitOrder { venue, buy, .. } => {
                assert_eq!(venue.chain, Some(ChainId("eip155:8453".into())));
                // zero-address output token → native.
                assert!(matches!(buy.key, TokenKey::Native { .. }));
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
    }
}
