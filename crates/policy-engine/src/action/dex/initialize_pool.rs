//! Pool-creation / initial-price action covering V3 createPool & V4 initialize.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRef, DecimalString};

use super::{HookPermissions, PoolRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Create a new pool or set its initial price.
#[allow(clippy::doc_markdown)]
pub struct InitializePoolAction {
    /// Pool identifier (V3 CREATE2-derived address or V4 PoolManager address + id).
    pub pool: PoolRef,
    /// Lower-address token in the pool.
    pub token0: AssetRef,
    /// Higher-address token in the pool.
    pub token1: AssetRef,
    /// Pool fee tier in hundredths of a basis point (raw `PoolKey.fee`).
    pub fee_bps: u32,
    /// V4 tick spacing, when present in calldata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_spacing: Option<i32>,
    /// Initial price as Q64.96 sqrt price, decimal-encoded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqrt_price_x96: Option<DecimalString>,
    /// V4 hook contract address from `PoolKey.hooks`, when non-zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Address>,
    /// Whether the pool is V4 dynamic-fee (`feeBps & 0x800000`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dynamic_fee: Option<bool>,
    /// Hook callback flags decoded from `hooks` address bits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_permissions: Option<HookPermissions>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::dex::test_support::{address, assert_roundtrip, decimal, erc20, pool};

    #[test]
    fn test_initialize_pool_action_serde_roundtrip_minimal() {
        let action = InitializePoolAction {
            pool: pool(),
            token0: erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
            token1: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            fee_bps: 500,
            tick_spacing: None,
            sqrt_price_x96: None,
            hooks: None,
            is_dynamic_fee: None,
            hook_permissions: None,
        };
        assert_roundtrip(&action);
    }

    #[test]
    fn test_initialize_pool_action_serde_roundtrip_full() {
        let action = InitializePoolAction {
            pool: pool(),
            token0: erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
            token1: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            fee_bps: 3000,
            tick_spacing: Some(60),
            sqrt_price_x96: Some(decimal("79228162514264337593543950336")),
            hooks: Some(address("0x9999999999999999999999999999999999999999")),
            is_dynamic_fee: Some(false),
            hook_permissions: Some(HookPermissions {
                before_swap: true,
                after_swap: true,
                ..HookPermissions::default()
            }),
        };
        assert_roundtrip(&action);
    }
}
