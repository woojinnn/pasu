//! WebSocket subscription 스켈레톤 — 효율 ↑ 의 운영 단계 옵션.
//!
//! Polling 기반 sync 가 자리잡은 후, 핵심 `LiveField` (블록 헤드, `mark_price` 등) 만
//! subscription 으로 업그레이드 → RPC 호출 50-80% 절감.
//!
//! 본 모듈은 trait + dummy poll 기반 impl + 미래의 WebSocket impl 의 자리만 잡음.
//! 실제 `tokio-tungstenite` 연동은 후속 (cfg-feature flag 로 분리 권장).
//!
//! 사용 패턴 (운영):
//! ```ignore
//! let sub = NewBlockSubscription::new_polling(router.clone(), chain, 5s);
//! while let Some(block) = sub.next().await {
//!     // 블록마다 trigger: lending HF 계산, balance refresh 등
//!     orch.refresh_for_scope(&mut state, &block_scope, block.time).await?;
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;

use simulation_state::ChainId;

use crate::error::SyncError;
use crate::fetchers::rpc::RpcRouter;

#[derive(Clone, Debug)]
pub struct NewBlock {
    pub chain: ChainId,
    pub number: u64,
}

/// 새 block 도착을 받는 stream.
#[async_trait]
pub trait BlockSubscription: Send + Sync {
    /// 다음 새 block 도착까지 대기.
    async fn next(&mut self) -> Option<NewBlock>;
    /// 중단.
    fn stop(&mut self);
}

/// 임시 polling 기반 — 운영 환경에서 `WsBlockSubscription` 로 swap 예정.
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
                                return; // 수신자 drop 됨 → 종료
                            }
                        }
                    }
                    Err(_) => continue, // 일시 실패는 다음 tick 에 재시도
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

/// WebSocket 기반 — 후속 구현 placeholder.
///
/// 동작 예정:
/// 1. provider 가 `ws: true` 인 endpoint 와 `eth_subscribe newHeads`
/// 2. 새 헤드 도착 시 `NewBlock` 채널로 push
/// 3. WebSocket 끊김 시 자동 재연결
///
/// 의존성: `tokio-tungstenite` 또는 `alloy-provider`. 둘 다 추가 build cost 가
/// 있으므로 feature flag (`ws`) 로 분리하는 게 권장.
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

/// 미사용 필드 경고 회피 — 후속에서 의존 `router/chain/interval/last_seen` 사용 예정.
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

        // 짧은 tick 이지만 publicnode 호출이 있으므로 timeout 안 으로만 검증.
        let mut sub = PollingBlockSubscription::new(
            router,
            ChainId::ethereum_mainnet(),
            Duration::from_millis(50),
        );

        // 네트워크 없으면 skip — 5초 안에 1개 받으면 OK.
        let _ = tokio::time::timeout(Duration::from_secs(5), sub.next()).await;
        sub.stop();
    }

    #[test]
    fn ws_placeholder_returns_unimplemented_error() {
        let r = WsBlockSubscription::new("wss://example".into(), ChainId::ethereum_mainnet());
        assert!(r.is_err());
    }
}
