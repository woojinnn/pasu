//! Restaking action schema types.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, Hex};

mod claim_restake_withdrawal;
mod request_restake_withdrawal;
mod restake;

pub use claim_restake_withdrawal::ClaimRestakeWithdrawalAction;
pub use request_restake_withdrawal::RequestRestakeWithdrawalAction;
pub use restake::RestakeAction;

/// Restaking strategy or vault reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyRef {
    /// Strategy or vault contract address, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Strategy or vault identifier, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Hex>,
    /// Human-readable strategy label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[cfg(test)]
pub(super) mod test_support {
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};
    use std::fmt::Debug;

    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn assert_json_roundtrip<T>(fixture: Value)
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let action = serde_json::from_value::<T>(fixture.clone()).unwrap();
        let serialized = serde_json::to_value(action).unwrap();

        assert_eq!(serialized, fixture);
    }

    pub(crate) fn address(value: u8) -> String {
        format!("0x{value:040x}")
    }

    pub(crate) fn hex32(value: u8) -> String {
        format!("0x{}", format!("{value:02x}").repeat(32))
    }

    pub(crate) fn native(symbol: &str) -> Value {
        json!({
            "kind": "native",
            "symbol": symbol,
            "decimals": 18
        })
    }

    pub(crate) fn erc20(symbol: &str) -> Value {
        json!({
            "kind": "erc20",
            "address": address(0x10),
            "symbol": symbol,
            "decimals": 18
        })
    }

    pub(crate) fn erc721(symbol: &str) -> Value {
        json!({
            "kind": "erc721",
            "address": address(0x11),
            "symbol": symbol
        })
    }

    pub(crate) fn amount(kind: &str, value: &str) -> Value {
        json!({ "kind": kind, "value": value })
    }

    pub(crate) fn strategy() -> Value {
        json!({
            "address": address(0x20),
            "id": hex32(0x21),
            "label": "Example Strategy"
        })
    }

    pub(crate) fn ticket() -> Value {
        json!({
            "nft": erc721("WITHDRAWAL"),
            "tokenId": "42",
            "id": hex32(0x22)
        })
    }
}
