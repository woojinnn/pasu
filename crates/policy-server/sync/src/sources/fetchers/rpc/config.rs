//! RPC 설정 — TOML 또는 환경변수에서 로드.
//!
//! 한 chain 당 N 개 provider 를 priority 순으로 정의. router 가 1순위 부터
//! 시도하다 실패 시 fallback. provider 종류는 `kind` 로 구분 ("public",
//! "alchemy", "infura", ...).

use std::collections::BTreeMap;
use std::path::Path;

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

use simulation_state::ChainId;

use crate::error::SyncError;

/// RPC providers + failover 정책.
///
/// 과거에는 TOML 의 최상위였으나 [`crate::SyncConfig`] 도입 후
/// `[rpc]` 섹션 아래로 들어간다. `RpcConfig::load_str` 은 기존 평탄
/// 포맷 (`[chains."..."]`) 도 그대로 받아들여 인라인 테스트 호환성을
/// 유지한다.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RpcConfig {
    /// chain id → 해당 체인의 provider 들.
    #[serde(default)]
    pub chains: BTreeMap<ChainId, ChainConfig>,

    #[serde(default)]
    pub failover: FailoverConfig,
}

/// 한 체인의 RPC 설정.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainConfig {
    /// 우선순위 순 provider 목록.
    pub providers: Vec<ProviderConfig>,
    /// Multicall3 컨트랙트 주소. None 이면 multicall 비활성화.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multicall_addr: Option<Address>,
}

/// 한 provider 의 설정.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 식별자 ("publicnode", "alchemy-eth", ...). router 가 이 이름으로 health 추적.
    pub name: String,
    /// provider 종류 — "public" / "alchemy" / "infura" / "quicknode" 등.
    pub kind: String,
    /// HTTP endpoint URL. `${ENV_VAR}` 형태 변수 치환 지원.
    pub url: String,
    /// 1 부터 시작 (1이 최우선).
    pub priority: u32,
    /// WebSocket subscription 지원 여부.
    #[serde(default)]
    pub ws: bool,
}

/// failover 전략 + 헬스 정책.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// "priority" | "`round_robin`" — 지금은 priority 만 구현.
    #[serde(default = "default_strategy")]
    pub strategy: String,
    /// 한 요청에 대해 시도할 최대 provider 수.
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,
    /// 시도 간 대기 (ms).
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    /// N번 연속 실패 시 unhealthy 마킹.
    #[serde(default = "default_mark_unhealthy_after")]
    pub mark_unhealthy_after: u32,
    /// unhealthy 유지 시간 (초). 이후 다시 시도.
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
    /// TOML 파일에서 로드. `${VAR}` 패턴은 환경변수로 치환.
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
