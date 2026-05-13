//! `BuildContext` — host-provided state every mapper needs.

use alloy_primitives::Address as AlloyAddress;

use crate::types::common::{Address, AssetKind, AssetRef};

/// Lower-case hex Address for an alloy address.
pub fn addr_to_string(a: AlloyAddress) -> Address {
    format!("0x{}", hex::encode(a.0 .0))
}

#[derive(Debug, Clone)]
pub struct RawTx {
    pub chain_id: u64,
    pub from: Address,
    pub to: Address,
    pub value: String,
    pub input: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct BuildContext {
    pub chain_id: u64,
    pub block_timestamp: i64,
    pub tokens: TokenRegistry,
}

#[derive(Debug, Clone, Default)]
pub struct TokenRegistry {}

impl TokenRegistry {
    pub fn erc20(&self, chain_id: u64, address: AlloyAddress) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            chain_id,
            address: Some(addr_to_string(address)),
            symbol: None,
            decimals: None,
        }
    }

    pub fn native(&self, chain_id: u64) -> AssetRef {
        AssetRef {
            kind: AssetKind::Native,
            chain_id,
            address: None,
            symbol: None,
            decimals: None,
        }
    }
}
