//! "alchemy", "infura", ...).

use std::collections::BTreeMap;
use std::path::Path;

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

use policy_state::ChainId;

use crate::error::SyncError;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RpcConfig {
    #[serde(default)]
    pub chains: BTreeMap<ChainId, ChainConfig>,

    #[serde(default)]
    pub failover: FailoverConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainConfig {
    pub providers: Vec<ProviderConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multicall_addr: Option<Address>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub kind: String,
    pub url: String,
    pub priority: u32,
    #[serde(default)]
    pub ws: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailoverConfig {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default = "default_mark_unhealthy_after")]
    pub mark_unhealthy_after: u32,
    #[serde(default = "default_unhealthy_cooldown_sec")]
    pub unhealthy_cooldown_sec: u64,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            retry_attempts: default_retry_attempts(),
            retry_delay_ms: default_retry_delay_ms(),
            mark_unhealthy_after: default_mark_unhealthy_after(),
            unhealthy_cooldown_sec: default_unhealthy_cooldown_sec(),
        }
    }
}

fn default_strategy() -> String {
    "priority".into()
}
const fn default_retry_attempts() -> u32 {
    3
}
const fn default_retry_delay_ms() -> u64 {
    200
}
const fn default_mark_unhealthy_after() -> u32 {
    5
}
const fn default_unhealthy_cooldown_sec() -> u64 {
    60
}

impl RpcConfig {
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self, SyncError> {
        let text = std::fs::read_to_string(&path).map_err(|e| SyncError::FetchFailed {
            source_id: "config_file".into(),
            reason: format!("{}: {}", path.as_ref().display(), e),
        })?;
        Self::load_str(&text)
    }

    pub fn load_str(text: &str) -> Result<Self, SyncError> {
        let expanded = crate::config::expand_env_vars(text);
        let cfg: Self = toml::from_str(&expanded).map_err(|e| SyncError::FetchFailed {
            source_id: "config_toml".into(),
            reason: e.to_string(),
        })?;
        Ok(cfg)
    }

    #[must_use]
    pub fn chain(&self, chain: &ChainId) -> Option<&ChainConfig> {
        self.chains.get(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let toml_text = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"

[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
ws = false
"#;
        let cfg = RpcConfig::load_str(toml_text).unwrap();
        let chain = cfg.chain(&ChainId::ethereum_mainnet()).unwrap();
        assert_eq!(chain.providers.len(), 1);
        assert_eq!(chain.providers[0].name, "publicnode");
        assert_eq!(chain.providers[0].priority, 1);
    }

    #[test]
    fn env_var_expansion() {
        std::env::set_var("TEST_RPC_KEY", "secret123");
        let toml_text = r#"
[[chains."eip155:1".providers]]
name = "alchemy"
kind = "alchemy"
url = "https://eth.alchemy.com/v2/${TEST_RPC_KEY}"
priority = 1
"#;
        let cfg = RpcConfig::load_str(toml_text).unwrap();
        let chain = cfg.chain(&ChainId::ethereum_mainnet()).unwrap();
        assert_eq!(
            chain.providers[0].url,
            "https://eth.alchemy.com/v2/secret123"
        );
    }
}
