//! Router — chain 별 provider 목록을 들고 priority 순서로 try, 실패 시
//! 다음 provider 로 fallback.
//!
//! 모든 fetcher (Onchain/Oracle/...) 가 직접 provider 를 안 부르고 router 만
//! 부른다. 그래서 provider 추가/교체가 호출자 코드 0 변경.

use std::collections::BTreeMap;
use std::sync::Arc;

use alloy_primitives::{Address, U256};
use tokio::sync::RwLock;

use simulation_state::ChainId;

use super::config::{ProviderConfig, RpcConfig};
use super::health::HealthTracker;
use super::providers::PublicRpcProvider;
use super::{BlockTag, EthCallRequest, RpcProvider};
use crate::error::SyncError;

/// 한 `RpcRouter` 는 여러 chain × 여러 provider 를 모두 관할.
pub struct RpcRouter {
    /// chain → priority 순 provider 목록.
    by_chain: BTreeMap<ChainId, Vec<Arc<dyn RpcProvider>>>,
    /// chain → multicall3 컨트랙트 (있을 때만).
    multicall: BTreeMap<ChainId, Address>,
    health: Arc<RwLock<HealthTracker>>,
}

impl std::fmt::Debug for RpcRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcRouter")
            .field("chains", &self.by_chain.keys().collect::<Vec<_>>())
            .field(
                "multicall_chains",
                &self.multicall.keys().collect::<Vec<_>>(),
            )
            .field("health", &"<HealthTracker>")
            .finish()
    }
}

impl RpcRouter {
    /// Config 에서 provider 인스턴스를 만들고 정렬.
    pub fn from_config(cfg: RpcConfig) -> Result<Self, SyncError> {
        let mut by_chain: BTreeMap<ChainId, Vec<Arc<dyn RpcProvider>>> = BTreeMap::new();
        let mut multicall: BTreeMap<ChainId, Address> = BTreeMap::new();

        for (chain, chain_cfg) in &cfg.chains {
            let mut providers = chain_cfg.providers.clone();
            providers.sort_by_key(|p| p.priority);

            let mut instances: Vec<Arc<dyn RpcProvider>> = Vec::with_capacity(providers.len());
            for p in providers {
                instances.push(instantiate_provider(chain.clone(), p)?);
            }
            by_chain.insert(chain.clone(), instances);

            if let Some(addr) = chain_cfg.multicall_addr {
                multicall.insert(chain.clone(), addr);
            }
        }

        Ok(Self {
            by_chain,
            multicall,
            health: Arc::new(RwLock::new(HealthTracker::new(cfg.failover))),
        })
    }

    #[must_use]
    pub fn multicall_addr(&self, chain: &ChainId) -> Option<Address> {
        self.multicall.get(chain).copied()
    }

    /// Every chain the router has at least one provider for. Stable
    /// order (BTreeMap). Used by `POST /wallets` to seed the wallet
    /// against every configured chain when the caller doesn't pin a
    /// `chains` set explicitly.
    pub fn chains(&self) -> impl Iterator<Item = &ChainId> + '_ {
        self.by_chain.keys()
    }

    async fn record(&self, provider_name: &str, ok: bool) {
        let mut health = self.health.write().await;
        if ok {
            health.record_success(provider_name);
        } else {
            health.record_failure(provider_name);
        }
    }

    /// 한 chain 의 모든 provider 를 priority 순으로 try.
    /// 첫 성공 시 즉시 반환, 모두 실패 시 마지막 에러.
    async fn try_all<F, Fut, T>(&self, chain: &ChainId, mut op: F) -> Result<T, SyncError>
    where
        F: FnMut(Arc<dyn RpcProvider>) -> Fut,
        Fut: std::future::Future<Output = Result<T, SyncError>>,
    {
        let providers = self
            .by_chain
            .get(chain)
            .ok_or_else(|| SyncError::FetchFailed {
                source_id: "router".into(),
                reason: format!("no providers for chain {chain}"),
            })?;

        let mut last_err: Option<SyncError> = None;
        for provider in providers {
            if self.health.read().await.is_unhealthy(provider.name()) {
                continue;
            }
            match op(provider.clone()).await {
                Ok(v) => {
                    self.record(provider.name(), true).await;
                    return Ok(v);
                }
                Err(e) => {
                    self.record(provider.name(), false).await;
                    last_err = Some(e);
                    continue;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| SyncError::FetchFailed {
            source_id: "router".into(),
            reason: format!("all providers failed for chain {chain}"),
        }))
    }

    // ============ public RPC 메서드 ============

    pub async fn eth_call(
        &self,
        chain: &ChainId,
        req: EthCallRequest,
    ) -> Result<Vec<u8>, SyncError> {
        self.try_all(chain, move |p| {
            let req = req.clone();
            async move { p.eth_call(req).await }
        })
        .await
    }

    pub async fn eth_balance(
        &self,
        chain: &ChainId,
        addr: Address,
        block: BlockTag,
    ) -> Result<U256, SyncError> {
        self.try_all(
            chain,
            move |p| async move { p.eth_balance(addr, block).await },
        )
        .await
    }

    pub async fn eth_block_number(&self, chain: &ChainId) -> Result<u64, SyncError> {
        self.try_all(chain, |p| async move { p.eth_block_number().await })
            .await
    }

    pub async fn eth_gas_price(&self, chain: &ChainId) -> Result<U256, SyncError> {
        self.try_all(chain, |p| async move { p.eth_gas_price().await })
            .await
    }

    pub async fn eth_get_transaction_receipt(
        &self,
        chain: &ChainId,
        tx_hash: &str,
    ) -> Result<Option<super::TxReceipt>, SyncError> {
        let hash = tx_hash.to_string();
        self.try_all(chain, move |p| {
            let h = hash.clone();
            async move { p.eth_get_transaction_receipt(&h).await }
        })
        .await
    }

    /// 안 healthy 한 provider 까지 한 번씩 강제 ping. cron 으로 호출.
    pub async fn health_sweep(&self) -> Vec<(String, Result<(), SyncError>)> {
        let mut results = Vec::new();
        for providers in self.by_chain.values() {
            for p in providers {
                let r = p.health_check().await;
                self.record(p.name(), r.is_ok()).await;
                results.push((p.name().to_string(), r));
            }
        }
        results
    }
}

fn instantiate_provider(
    chain: ChainId,
    cfg: ProviderConfig,
) -> Result<Arc<dyn RpcProvider>, SyncError> {
    match cfg.kind.as_str() {
        "public" => Ok(Arc::new(PublicRpcProvider::new(cfg.name, chain, cfg.url))),
        // 향후 추가:
        // "alchemy"   => Ok(Arc::new(AlchemyProvider::new(...))),
        // "infura"    => Ok(Arc::new(InfuraProvider::new(...))),
        // "quicknode" => Ok(Arc::new(QuickNodeProvider::new(...))),
        other => Err(SyncError::FetchFailed {
            source_id: "router".into(),
            reason: format!("unknown provider kind: {other}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_cfg() -> RpcConfig {
        let toml_text = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"

[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        RpcConfig::load_str(toml_text).unwrap()
    }

    #[test]
    fn router_builds_from_config() {
        let router = RpcRouter::from_config(minimal_cfg()).unwrap();
        let chain = ChainId::ethereum_mainnet();
        assert!(router.by_chain.contains_key(&chain));
        assert_eq!(router.by_chain[&chain].len(), 1);
        assert!(router.multicall_addr(&chain).is_some());
    }

    #[test]
    fn router_rejects_unknown_provider_kind() {
        let toml_text = r#"
[[chains."eip155:1".providers]]
name = "x"
kind = "unknown_provider"
url = "https://example.com"
priority = 1
"#;
        let cfg = RpcConfig::load_str(toml_text).unwrap();
        let err = RpcRouter::from_config(cfg).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown provider kind"));
    }
}
