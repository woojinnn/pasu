//! RPC layer — JSON-RPC over HTTP 추상화.
//!
//! 구성:
//! * [`RpcProvider`] trait — 한 provider 의 메서드 셋
//! * [`RpcRouter`]         — 여러 provider 의 failover wrapper
//! * [`config`]            — TOML 로 endpoint 정의
//! * [`providers`]         — 구체 구현 (publicnode, alchemy, infura ...)
//! * [`multicall`]         — Multicall3 wrapper (배치 eth_call)

use alloy_primitives::{Address, U256};
use async_trait::async_trait;

use simulation_state::ChainId;

use crate::error::SyncError;

pub mod config;
pub mod health;
pub mod multicall;
pub mod providers;
pub mod router;
pub mod rpc_types;

pub use config::{ProviderConfig, RpcConfig};
pub use rpc_types::{BlockTag, EthCallRequest, ProviderName};
pub use router::RpcRouter;

/// 한 RPC provider 의 표준 인터페이스.
///
/// 모든 provider 는 같은 메서드 셋을 노출. failover wrapper ([`RpcRouter`])
/// 가 이 trait 만 보고 동작하므로, 새 provider 추가 = trait 구현 1개.
#[async_trait]
pub trait RpcProvider: Send + Sync {
    fn name(&self) -> &str;
    fn chain(&self) -> &ChainId;
    fn supports_websocket(&self) -> bool {
        false
    }

    /// 헬스 체크 — 단순한 read 호출로 connectivity 확인.
    async fn health_check(&self) -> Result<(), SyncError>;

    /// `eth_call` — view 함수 호출.
    async fn eth_call(&self, req: EthCallRequest) -> Result<Vec<u8>, SyncError>;

    /// `eth_getBalance` — native 잔고.
    async fn eth_balance(&self, address: Address, block: BlockTag) -> Result<U256, SyncError>;

    /// `eth_blockNumber` — 현재 체인 head.
    async fn eth_block_number(&self) -> Result<u64, SyncError>;

    /// `eth_gasPrice` — 현재 gas price.
    async fn eth_gas_price(&self) -> Result<U256, SyncError>;
}
