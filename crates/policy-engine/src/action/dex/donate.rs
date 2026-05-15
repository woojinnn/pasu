//! V4 donate action — push assets into a pool's in-range LPs without minting.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRefWithAmountConstraint, Hex, Validity};

use super::{HookPermissions, PoolRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Push assets into a Uniswap V4 pool's in-range LPs (no LP token minted).
pub struct DonateAction {
    /// Target pool.
    pub pool: PoolRef,
    /// Donated assets paired with their amount constraints (currency0, currency1).
    #[serde(rename = "inputTokens")]
    pub input_tokens: Vec<AssetRefWithAmountConstraint>,
    /// Originating wallet (`msg.sender` of the V4 donate call), when derivable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// Validity window, when present in calldata or wrapper context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
    /// Hook contract address from `PoolKey.hooks`, when non-zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Address>,
    /// Hook callback flags decoded from `hooks` address bits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_permissions: Option<HookPermissions>,
    /// Whether the pool is V4 dynamic-fee (`PoolKey.fee & 0x800000`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dynamic_fee: Option<bool>,
    /// Length of the trailing `hookData` payload, in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_data_len: Option<u64>,
    /// First four bytes of `hookData` (selector-like), when length ≥ 4.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_data_selector: Option<Hex>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::AmountKind;
    use crate::action::dex::test_support::{
        address, assert_roundtrip, asset_amount_pair, hex, pool, validity,
    };

    #[test]
    fn test_donate_action_serde_roundtrip_minimal() {
        let action = DonateAction {
            pool: pool(),
            input_tokens: asset_amount_pair(AmountKind::Exact, AmountKind::Exact),
            from: None,
            validity: None,
            hooks: None,
            hook_permissions: None,
            is_dynamic_fee: None,
            hook_data_len: None,
            hook_data_selector: None,
        };
        assert_roundtrip(&action);
    }

    #[test]
    fn test_donate_action_serde_roundtrip_full() {
        let action = DonateAction {
            pool: pool(),
            input_tokens: asset_amount_pair(AmountKind::Exact, AmountKind::Exact),
            from: Some(address("0x2222222222222222222222222222222222222222")),
            validity: Some(validity()),
            hooks: Some(address("0x9999999999999999999999999999999999999999")),
            hook_permissions: Some(HookPermissions {
                before_donate: true,
                after_donate: true,
                ..HookPermissions::default()
            }),
            is_dynamic_fee: Some(false),
            hook_data_len: Some(36),
            hook_data_selector: Some(hex("0x12345678")),
        };
        assert_roundtrip(&action);
    }
}
