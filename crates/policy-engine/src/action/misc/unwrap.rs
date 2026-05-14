//! Unwrap action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

/// Unwrap a wrapped ERC-20 asset into its native representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnwrapAction {
    /// Wrapped ERC-20 asset being burned.
    pub wrapped_asset: AssetRef,
    /// Native asset being received.
    pub native_asset: AssetRef,
    /// Unwrap amount.
    pub amount: AmountConstraint,
    /// Recipient of the native asset.
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, amount, assert_json_roundtrip, erc20, native};
    use serde_json::json;

    #[test]
    fn test_unwrap_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<UnwrapAction>(json!({
            "wrappedAsset": erc20("WETH"),
            "nativeAsset": native("ETH"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }

    #[test]
    fn test_unwrap_action_serde_roundtrip_full() {
        assert_json_roundtrip::<UnwrapAction>(json!({
            "wrappedAsset": erc20("WETH"),
            "nativeAsset": native("ETH"),
            "amount": amount("min", "900"),
            "recipient": address(0x31)
        }));
    }
}
