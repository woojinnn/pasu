//! Generic REST JSON oracle fetcher.
//!
//! 한 provider 의 endpoint + feed catalog 만 받으면 동작 — 새 oracle 추가는
//! `scopeball-sync.toml` 의 `[oracles.rest.<name>]` 한 블록만 늘리면 끝.
//!
//! 동작:
//! 1. `DataSource::OracleFeed.feed_id` 로 catalog 의 [`RestFeedConfig`] 룩업
//! 2. `base_url + path` 로 `GET`. auth 가 있으면 헤더 부착.
//! 3. JSON 응답에서 `json_pointer` (RFC 6901) 로 가격 숫자 추출.
//! 4. `Decimal` 로 wrap 해서 반환.
//!
//! 지원 가격 형태:
//! * `Value::Number` — 그대로 문자열화.
//! * `Value::String` — 그대로 사용 (CoinMarketCap 처럼 string 으로 주는 경우).
//! 그 외는 `FetchFailed`.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use simulation_state::{DataSource, Decimal};

use crate::config::RestOracleConfig;
use crate::error::SyncError;
use crate::fetchers::oracle::PriceFetcher;

/// 한 REST oracle (예: CoinGecko) 의 fetcher.
#[derive(Debug)]
pub struct RestJsonOracleFetcher {
    /// 에러 메시지 / 로깅용 — provider 의 canonical name.
    name: String,
    client: reqwest::Client,
    base_url: String,
    /// `(header_name, value)` — value 는 생성 시점에 env 에서 한 번 resolve.
    auth: Option<(String, String)>,
    /// feed_id → (path, json_pointer).
    feeds: HashMap<String, RestFeedSpec>,
}

#[derive(Debug, Clone)]
struct RestFeedSpec {
    path: String,
    json_pointer: String,
}

impl RestJsonOracleFetcher {
    /// `scopeball-sync.toml` 의 `[oracles.rest.<name>]` 한 블록을 받아 build.
    ///
    /// `name` 은 dispatch HashMap 의 키 ( = `OracleProvider::Other(name)` 와 매칭).
    /// 일반적으로 TOML 의 섹션 이름과 같다 (예: `"coingecko"`).
    ///
    /// auth.env_var 가 비어있거나 환경변수가 없으면 인증 없이 호출.
    pub fn from_sync_config(name: impl Into<String>, cfg: &RestOracleConfig) -> Self {
        let auth = cfg.auth.as_ref().and_then(|a| {
            let value = std::env::var(&a.env_var).ok()?;
            if value.is_empty() {
                None
            } else {
                Some((a.header_name.clone(), value))
            }
        });
        let feeds = cfg
            .feeds
            .iter()
            .map(|(id, f)| {
                (
                    id.clone(),
                    RestFeedSpec {
                        path: f.path.clone(),
                        json_pointer: f.json_pointer.clone(),
                    },
                )
            })
            .collect();
        Self {
            name: name.into(),
            // CoinGecko 등 일부 API 가 명시적 User-Agent 를 요구함 — 기본 hyper UA
            // 면 403. 모든 REST oracle 호출에 scopeball 식별자를 박아둠.
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_sec))
                .user_agent(concat!("scopeball-sync/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client init"),
            base_url: cfg.base_url.clone(),
            auth,
            feeds,
        }
    }

    /// provider name — 주로 디버깅용.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    fn err(&self, reason: impl Into<String>) -> SyncError {
        SyncError::FetchFailed {
            source_id: format!("rest_oracle:{}", self.name),
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl PriceFetcher for RestJsonOracleFetcher {
    async fn fetch_price(&self, source: &DataSource) -> Result<Decimal, SyncError> {
        // 1) source 검증 + feed_id 추출
        let feed_id = match source {
            DataSource::OracleFeed { feed_id, .. } => feed_id.clone(),
            _ => return Err(self.err("not an OracleFeed source")),
        };

        // 2) catalog 룩업
        let spec = self
            .feeds
            .get(&feed_id)
            .ok_or_else(|| self.err(format!("unknown feed_id: {feed_id}")))?;

        // 3) HTTP GET
        let url = format!("{}{}", self.base_url, spec.path);
        let mut req = self.client.get(&url);
        if let Some((hname, hval)) = &self.auth {
            req = req.header(hname, hval);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| self.err(format!("http get {url}: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(self.err(format!("status {status}: {body}")));
        }

        // 4) JSON 파싱
        let body: Value = resp
            .json()
            .await
            .map_err(|e| self.err(format!("json decode {url}: {e}")))?;

        // 5) JSON pointer 로 가격 꺼내기
        let extracted = body
            .pointer(&spec.json_pointer)
            .ok_or_else(|| self.err(format!("pointer {} missing in {body}", spec.json_pointer)))?;

        let price_str = match extracted {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            other => {
                return Err(self.err(format!(
                    "pointer {} resolved to non-numeric: {other}",
                    spec.json_pointer
                )));
            }
        };

        Ok(Decimal::new(price_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RestAuthConfig, RestFeedConfig};
    use std::collections::BTreeMap;

    fn cfg_no_auth() -> RestOracleConfig {
        let mut feeds = BTreeMap::new();
        feeds.insert(
            "USDC/USD".into(),
            RestFeedConfig {
                path: "/simple/price?ids=usd-coin&vs_currencies=usd".into(),
                json_pointer: "/usd-coin/usd".into(),
            },
        );
        RestOracleConfig {
            base_url: "http://localhost:0".into(),
            auth: None,
            timeout_sec: 1,
            feeds,
        }
    }

    #[test]
    fn ignores_unknown_feed() {
        let f = RestJsonOracleFetcher::from_sync_config("coingecko", &cfg_no_auth());
        assert_eq!(f.name(), "coingecko");
        assert!(f.feeds.contains_key("USDC/USD"));
    }

    #[tokio::test]
    async fn rejects_non_oracle_source() {
        use simulation_state::ChainId;
        let f = RestJsonOracleFetcher::from_sync_config("coingecko", &cfg_no_auth());
        let bad = DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: alloy_primitives::Address::ZERO,
            function: "x()".into(),
            decoder_id: "noop".into(),
        };
        let err = f.fetch_price(&bad).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not an OracleFeed"), "msg={msg}");
    }

    #[tokio::test]
    async fn unknown_feed_id_errors() {
        use simulation_state::OracleProvider;
        let f = RestJsonOracleFetcher::from_sync_config("coingecko", &cfg_no_auth());
        let src = DataSource::OracleFeed {
            provider: OracleProvider::Other("coingecko".into()),
            feed_id: "UNKNOWN/USD".into(),
        };
        let err = f.fetch_price(&src).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown feed_id"), "msg={msg}");
    }

    #[test]
    fn auth_resolved_from_env_at_build_time() {
        std::env::set_var("CGTEST_KEY", "k123");
        let mut cfg = cfg_no_auth();
        cfg.auth = Some(RestAuthConfig {
            header_name: "X-CG-Test".into(),
            env_var: "CGTEST_KEY".into(),
        });
        let f = RestJsonOracleFetcher::from_sync_config("coingecko", &cfg);
        assert_eq!(
            f.auth.as_ref().map(|(h, v)| (h.clone(), v.clone())),
            Some(("X-CG-Test".to_string(), "k123".to_string()))
        );
    }

    #[test]
    fn auth_omitted_when_env_missing() {
        std::env::remove_var("CGTEST_KEY_MISSING");
        let mut cfg = cfg_no_auth();
        cfg.auth = Some(RestAuthConfig {
            header_name: "X-CG-Test".into(),
            env_var: "CGTEST_KEY_MISSING".into(),
        });
        let f = RestJsonOracleFetcher::from_sync_config("coingecko", &cfg);
        assert!(f.auth.is_none());
    }
}
