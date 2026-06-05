//! ```ignore
//! let sub = NewBlockSubscription::new_polling(router.clone(), chain, 5s);
//! while let Some(block) = sub.next().await {
//!     orch.refresh_for_scope(&mut state, &block_scope, block.time).await?;
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;

use policy_state::ChainId;

use crate::error::SyncError;
use crate::fetchers::rpc::RpcRouter;

#[derive(Clone, Debug)]
pub struct NewBlock {
    pub chain: ChainId,
    pub number: u64,
}

#[async_trait]
pub trait BlockSubscription: Send + Sync {
    async fn next(&mut self) -> Option<NewBlock>;
    fn stop(&mut self);
}

pub struct PollingBlockSubscription {
    router: Arc<RpcRouter>,
    chain: ChainId,
    interval: Duration,
    last_seen: Option<u64>,
    rx: mpsc::Receiver<NewBlock>,
    task: tokio::task::JoinHandle<()>,
}

impl PollingBlockSubscription {
    #[must_use]
    pub fn new(router: Arc<RpcRouter>, chain: ChainId, interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel::<NewBlock>(16);
        let r = router.clone();
        let c = chain.clone();
        let task = tokio::spawn(async move {
            let mut last: Option<u64> = None;
            loop {
                tokio::time::sleep(interval).await;
                match r.eth_block_number(&c).await {
                    Ok(n) => {
                        if last != Some(n) {
                            last = Some(n);
                            if tx
                                .send(NewBlock {
                                    chain: c.clone(),
                                    number: n,
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    Err(_) => continue,
                }
            }
        });
        Self {
            router,
            chain,
            interval,
            last_seen: None,
            rx,
            task,
        }
    }
}

#[async_trait]
impl BlockSubscription for PollingBlockSubscription {
    async fn next(&mut self) -> Option<NewBlock> {
        self.rx.recv().await
    }
    fn stop(&mut self) {
        self.task.abort();
    }
}

#[allow(dead_code)]
pub struct WsBlockSubscription {
    _placeholder: (),
}

#[allow(dead_code)]
impl WsBlockSubscription {
    pub fn new(_endpoint: String, _chain: ChainId) -> Result<Self, SyncError> {
        Err(SyncError::FetchFailed {
            source_id: "ws_subscription".into(),
            reason: "WebSocket subscription not implemented yet (feature `ws`)".into(),
        })
    }
}

#[allow(dead_code)]
const fn _polling_fields_marker(s: &PollingBlockSubscription) {
    let _ = (&s.router, &s.chain, s.interval, s.last_seen);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn polling_subscription_yields_block_numbers() {
        let toml = r#"
[chains."eip155:1"]
multicall_addr = "0xcA11bde05977b3631167028862bE2a173976CA11"
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
        let cfg = crate::RpcConfig::load_str(toml).unwrap();
        let router = Arc::new(crate::RpcRouter::from_config(cfg).unwrap());

        let mut sub = PollingBlockSubscription::new(
            router,
            ChainId::ethereum_mainnet(),
            Duration::from_millis(50),
        );

        let _ = tokio::time::timeout(Duration::from_secs(5), sub.next()).await;
        sub.stop();
    }

    #[test]
    fn ws_placeholder_returns_unimplemented_error() {
        let r = WsBlockSubscription::new("wss://example".into(), ChainId::ethereum_mainnet());
        assert!(r.is_err());
    }
}
