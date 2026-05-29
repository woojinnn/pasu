//! Hyperliquid REST fetcher — mark price, funding rate, open orders.
//!
//! API: https://api.hyperliquid.xyz/info  (POST JSON body)
//!
//! `DataSource::VenueApi { endpoint, parser_id, .. }` 의 parser_id 로 메서드 식별:
//! - `hl_all_mids`         → 전체 mark price (key=coin symbol, val=price string)
//! - `hl_funding`          → 각 perp 의 funding rate
//! - `hl_open_orders`      → 한 유저의 미체결 주문 lifecycle 추적
//!
//! body 는 endpoint URL 옆에 별도로 들고와야 하지만, parser_id 가 같으면 body 구조도
//! 같다는 가정 하에 간단한 매핑 테이블로 처리.

use std::time::Duration;

use serde_json::{Value, json};

use simulation_state::DataSource;

use crate::config::HyperliquidConfig;
use crate::error::SyncError;

/// Hyperliquid API 기본 endpoint. `scopeball-sync.toml` 의
/// `[venues.hyperliquid]` 가 비어있을 때 fallback 으로 사용.
pub const HL_API_BASE: &str = "https://api.hyperliquid.xyz";

pub struct HyperliquidFetcher {
    client: reqwest::Client,
    /// venue API 의 base URL. `DataSource::VenueApi.endpoint` 가 절대 URL 이면
    /// 그쪽이 우선; relative path 만 들어올 경우를 대비한 base.
    base_url: String,
}

impl Default for HyperliquidFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HyperliquidFetcher {
    /// 기본 endpoint (`HL_API_BASE`) 로 초기화.
    pub fn new() -> Self {
        Self::with_base_url(HL_API_BASE.to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            base_url,
        }
    }

    /// `scopeball-sync.toml` 의 `[venues.hyperliquid]` 섹션에서 endpoint 주입.
    pub fn from_sync_config(cfg: &HyperliquidConfig) -> Self {
        Self::with_base_url(cfg.endpoint.clone())
    }

    /// 현재 설정된 base URL — 호출자가 endpoint 결정 시 참고용.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn fetch(&self, source: &DataSource) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = match source {
            DataSource::VenueApi {
                endpoint,
                parser_id,
                ..
            } => (endpoint.clone(), parser_id.clone()),
            _ => {
                return Err(SyncError::FetchFailed {
                    source_id: "hyperliquid".into(),
                    reason: "not a VenueApi source".into(),
                });
            }
        };

        let body = match parser_id.as_str() {
            "hl_all_mids" => json!({ "type": "allMids" }),
            "hl_funding" => json!({ "type": "metaAndAssetCtxs" }),
            "hl_open_orders" => {
                // open orders 는 user 필요. parser_id 만으로는 부족 — endpoint 의
                // path 에 user 가 박혀있다고 가정. 또는 별도 메타 필드.
                // 여기는 stub — 빈 user 로 호출 (실제 사용 시 endpoint 에 user 포함).
                json!({ "type": "openOrders", "user": "0x0000000000000000000000000000000000000000" })
            }
            other => {
                return Err(SyncError::FetchFailed {
                    source_id: "hyperliquid".into(),
                    reason: format!("unknown parser_id: {}", other),
                });
            }
        };

        let url = if endpoint.is_empty() {
            format!("{}/info", HL_API_BASE)
        } else {
            endpoint
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("http: {}", e),
            })?;

        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("status {}", resp.status()),
            });
        }

        let value: Value = resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: format!("json: {}", e),
        })?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::DataSource;

    #[test]
    fn rejects_non_venue_source() {
        let f = HyperliquidFetcher::new();
        let bad = DataSource::UserSupplied;
        let res = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(f.fetch(&bad));
        assert!(res.is_err());
    }

    #[test]
    fn rejects_unknown_parser() {
        let f = HyperliquidFetcher::new();
        let bad = DataSource::VenueApi {
            endpoint: HL_API_BASE.into(),
            parser_id: "made_up".into(),
            auth: None,
        };
        let res = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(f.fetch(&bad));
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("unknown parser_id"));
    }
}
