//! Uniswap Trade API fetcher — UniswapX order lifecycle (`GET /v1/orders`).
//! API: <https://trade-api.gateway.uniswap.org/v1/orders> (header `x-api-key`).

use std::str::FromStr;
use std::time::Duration;

use policy_state::pending::{
    AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::{Address, ChainId, Time, U256, VenueRef};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::{DataSource, StateDelta};

use crate::config::UniswapConfig;
use crate::error::SyncError;

/// Fetches UniswapX order status from the Uniswap Trade API (`GET /v1/orders`).
pub struct UniswapXFetcher {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    chains: Vec<ChainId>,
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
            chains: cfg.chains.clone(),
        }
    }

    #[must_use]
    pub fn chains(&self) -> &[ChainId] {
        &self.chains
    }

    /// `GET {base}/orders?swapper={swapper}&chainId={n}&limit=50`, following
    /// `cursor` pages, returning the decoded orders for one chain. Non-eip155
    /// chains are not on the Trade API and yield an empty list.
    pub async fn fetch_orders(
        &self,
        swapper: &Address,
        chain: &ChainId,
    ) -> Result<Vec<UniswapXOrder>, SyncError> {
        let Some(chain_num) = eip155_chain_id(chain) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut url = format!(
                "{}/orders?swapper={:#x}&chainId={chain_num}&limit=50",
                self.base_url.trim_end_matches('/'),
                swapper,
            );
            if let Some(c) = &cursor {
                url.push_str(&format!("&cursor={c}"));
            }
            let resp = self
                .client
                .get(&url)
                .header("x-api-key", &self.api_key)
                .send()
                .await
                .map_err(|e| SyncError::FetchFailed {
                    source_id: "uniswap_x".into(),
                    reason: format!("http: {e}"),
                })?;
            if !resp.status().is_success() {
                return Err(SyncError::FetchFailed {
                    source_id: "uniswap_x".into(),
                    reason: format!("status {}", resp.status()),
                });
            }
            let body: serde_json::Value =
                resp.json().await.map_err(|e| SyncError::FetchFailed {
                    source_id: "uniswap_x".into(),
                    reason: format!("decode: {e}"),
                })?;
            if let Some(page) = parse_orders(&body) {
                out.extend(page);
            }
            cursor = body
                .get("cursor")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }
}

/// `eip155:<n>` → `n`. Returns `None` for non-eip155 CAIP-2 ids.
fn eip155_chain_id(chain: &ChainId) -> Option<u64> {
    chain.0.strip_prefix("eip155:")?.parse().ok()
}

/// Map a Uniswap Trade API `orderStatus` string to our canonical
/// `PendingStatus`. The second tuple element is the verbatim venue string,
/// stored in `PendingLifecycle.raw_status`.
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

/// One UniswapX order as returned by `GET /v1/orders`. Only the fields we
/// project into state are decoded; unknown fields are ignored.
#[derive(Debug, Clone)]
pub struct UniswapXOrder {
    pub order_hash: String,
    pub order_status: String,
    pub order_type: String,
    pub deadline: u64,
    pub sell_token: Address,
    pub sell_amount: U256,
    pub buy_token: Address,
    pub buy_min: U256,
}

/// Decode the `{ "orders": [...] }` envelope into `UniswapXOrder`s, skipping
/// any element that lacks a decodable `orderHash` / `input` / `outputs`.
#[must_use]
pub fn parse_orders(value: &serde_json::Value) -> Option<Vec<UniswapXOrder>> {
    let arr = value.get("orders")?.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for o in arr {
        if let Some(order) = parse_one(o) {
            out.push(order);
        }
    }
    Some(out)
}

fn parse_one(o: &serde_json::Value) -> Option<UniswapXOrder> {
    let order_hash = o.get("orderHash")?.as_str()?.to_owned();
    let order_status = o.get("orderStatus")?.as_str()?.to_owned();
    let order_type = o
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let deadline = o
        .get("deadline")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let input = o.get("input")?;
    let sell_token = Address::from_str(input.get("token")?.as_str()?).ok()?;
    // Input is fixed-or-decaying; the seller's worst case is the larger end.
    let sell_amount = u256_dec(input.get("startAmount")?.as_str()?)
        .max(u256_dec(input.get("endAmount")?.as_str()?));

    // Pick the first output (paid to the swapper); the guaranteed minimum is
    // the smaller of its decay endpoints (`endAmount`).
    let outputs = o.get("outputs")?.as_array()?;
    let primary = outputs.first()?;
    let buy_token = Address::from_str(primary.get("token")?.as_str()?).ok()?;
    let buy_min = u256_dec(primary.get("startAmount")?.as_str()?)
        .min(u256_dec(primary.get("endAmount")?.as_str()?));

    Some(UniswapXOrder {
        order_hash,
        order_status,
        order_type,
        deadline,
        sell_token,
        sell_amount,
        buy_token,
        buy_min,
    })
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

fn token_ref(chain: &ChainId, address: Address) -> TokenRef {
    // UniswapX uses the zero / 0xeee… sentinel for native ETH.
    let native = Address::from_str("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
        .unwrap_or(Address::ZERO);
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
    /// Project this order into a `PendingTx`. `reactor` is the UniswapX reactor
    /// (the permit-cap spender). The venue `orderHash` is embedded in the id so
    /// upserts are idempotent and a future reducer-reconciliation can join on it.
    #[must_use]
    pub fn to_pending_tx(
        &self,
        chain: &ChainId,
        reactor: Address,
        _swapper: &Address,
        now: Time,
    ) -> PendingTx {
        let (status, raw_status) = map_uniswapx_status(&self.order_status);
        let sell = token_ref(chain, self.sell_token);
        let buy = token_ref(chain, self.buy_token);

        PendingTx {
            id: format!("intent:uniswap_x:{}", self.order_hash),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef {
                    name: "uniswap_x".into(),
                    chain: Some(chain.clone()),
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
                valid_until: Some(Time::from_unix(self.deadline)),
                nonce: None, // orderHash lives in `id`; typed nonce wiring deferred
                on_chain_tx: None,
                raw_status,
            },
            sync: DataSource::VenueApi {
                endpoint: "https://trade-api.gateway.uniswap.org/v1".into(),
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

    const SAMPLE_ORDERS: &str = r#"{
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
    fn parses_orders_into_pending_tx() {
        let swapper = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
        let chain = ChainId::ethereum_mainnet();
        let reactor = Address::from_str("0x6000da47483062A0D734Ba3dc7576Ce6A0b645c4").unwrap();
        let value: serde_json::Value = serde_json::from_str(SAMPLE_ORDERS).unwrap();

        let orders = parse_orders(&value).unwrap();
        assert_eq!(orders.len(), 1);

        let now = Time::from_unix(1_738_000_000);
        let p = orders[0].to_pending_tx(&chain, reactor, &swapper, now);

        assert_eq!(p.id, "intent:uniswap_x:0xabc123");
        assert_eq!(p.lifecycle.status, PendingStatus::Active);
        assert_eq!(p.lifecycle.raw_status.as_deref(), Some("open"));
        assert_eq!(p.lifecycle.valid_until, Some(Time::from_unix(1_738_003_600)));
        match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue,
                sell,
                buy,
                sell_max,
                buy_min,
                order_kind,
            } => {
                assert_eq!(venue.name, "uniswap_x");
                assert_eq!(*order_kind, OrderKind::Dutch);
                assert_eq!(*sell_max, U256::from(600_000_000u64)); // input amount
                assert_eq!(*buy_min, U256::from(295_000_000_000_000_000u64)); // output endAmount
                assert!(matches!(sell.key, TokenKey::Erc20 { .. }));
                assert!(matches!(buy.key, TokenKey::Erc20 { .. }));
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
        match &p.commitment {
            AssetCommitment::PermitCap {
                spender, max_out, ..
            } => {
                assert_eq!(*spender, reactor);
                assert_eq!(*max_out, U256::from(600_000_000u64));
            }
            other => panic!("expected PermitCap, got {other:?}"),
        }
    }
}
