//! Generic REST JSON oracle fetcher.
use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use policy_state::{DataSource, Decimal};

use crate::config::RestOracleConfig;
use crate::error::SyncError;
use crate::fetchers::oracle::PriceFetcher;

#[derive(Debug)]
pub struct RestJsonOracleFetcher {
    name: String,
    client: reqwest::Client,
    base_url: String,
    auth: Option<(String, String)>,
    /// `feed_id` → (path, `json_pointer`).
    feeds: HashMap<String, RestFeedSpec>,
}

#[derive(Debug, Clone)]
struct RestFeedSpec {
    path: String,
    json_pointer: String,
}

impl RestJsonOracleFetcher {
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
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(cfg.timeout_sec))
                .user_agent(concat!("dambi-sync/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client init"),
            base_url: cfg.base_url.clone(),
            auth,
            feeds,
        }
    }

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
        let feed_id = match source {
            DataSource::OracleFeed { feed_id, .. } => feed_id.clone(),
            _ => return Err(self.err("not an OracleFeed source")),
        };

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

        let body: Value = resp
            .json()
            .await
            .map_err(|e| self.err(format!("json decode {url}: {e}")))?;

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
        use policy_state::ChainId;
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
        use policy_state::OracleProvider;
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
