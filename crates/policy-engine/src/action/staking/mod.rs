//! Staking action schema types.

use serde::{Deserialize, Serialize};

use crate::action::common::{AssetRef, DecimalString, Hex};

mod claim_unstake;
mod request_unstake;
mod stake;

pub use claim_unstake::ClaimUnstakeAction;
pub use request_unstake::RequestUnstakeAction;
pub use stake::StakeAction;

/// Claim ticket for a delayed unstake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketRef {
    /// Ticket NFT collection, when the claim right is represented as an NFT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nft: Option<AssetRef>,
    /// Ticket token id or sequence id, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
    /// Bytes identifier, when the ticket is hash-based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Hex>,
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

    pub(crate) fn ticket() -> Value {
        json!({
            "nft": erc721("WITHDRAWAL"),
            "tokenId": "42",
            "id": hex32(0x20)
        })
    }
}
