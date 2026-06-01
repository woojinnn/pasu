use alloy_primitives::{Address, U256};
use async_trait::async_trait;

use policy_state::ChainId;

use crate::error::SyncError;

pub mod config;
pub mod health;
pub mod multicall;
pub mod providers;
pub mod router;
pub mod rpc_types;

pub use config::{ProviderConfig, RpcConfig};
pub use router::RpcRouter;
pub use rpc_types::{BlockTag, EthCallRequest, ProviderName};

#[async_trait]
pub trait RpcProvider: Send + Sync {
    fn name(&self) -> &str;
    fn chain(&self) -> &ChainId;
    fn supports_websocket(&self) -> bool {
        false
    }

    async fn health_check(&self) -> Result<(), SyncError>;

    async fn eth_call(&self, req: EthCallRequest) -> Result<Vec<u8>, SyncError>;

    async fn eth_balance(&self, address: Address, block: BlockTag) -> Result<U256, SyncError>;

    async fn eth_block_number(&self) -> Result<u64, SyncError>;

    async fn eth_gas_price(&self) -> Result<U256, SyncError>;

    async fn eth_get_transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> Result<Option<TxReceipt>, SyncError>;
}

#[derive(Debug, Clone)]
pub struct TxReceipt {
    /// `1` = success, `0` = revert.
    pub status: bool,
    pub block_number: u64,
    pub block_hash: String,
    pub gas_used: u64,
    pub tx_hash: String,
}
