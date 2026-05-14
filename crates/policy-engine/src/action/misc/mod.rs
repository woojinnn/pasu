//! Miscellaneous action schema types.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, Hex};

mod approve;
mod claim_rewards;
mod delegate;
mod permit;
mod set_approval_for_all;
mod sign_message;
mod transfer;
mod unwrap;
mod vote;
mod wrap;

pub use approve::ApproveAction;
pub use claim_rewards::ClaimRewardsAction;
pub use delegate::DelegateAction;
pub use permit::PermitAction;
pub use set_approval_for_all::SetApprovalForAllAction;
pub use sign_message::SignMessageAction;
pub use transfer::TransferAction;
pub use unwrap::UnwrapAction;
pub use vote::VoteAction;
pub use wrap::WrapAction;

/// ERC-20 approval variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    /// Standard ERC-20 approval.
    Erc20,
    /// ERC-20 allowance increase.
    Erc20Increase,
    /// ERC-20 allowance decrease.
    Erc20Decrease,
    /// Uniswap Permit2 approval.
    Permit2,
}

/// Permit signature variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermitKind {
    /// EIP-2612 token permit.
    Eip2612,
    /// Permit2 allowance grant.
    Permit2Single,
    /// Permit2 one-shot transfer authorization.
    Permit2Transfer,
}

/// Governance power type for split-power tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerType {
    /// Voting power.
    Voting,
    /// Proposal creation power.
    Proposition,
}

/// Governance vote support direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteSupport {
    /// Vote in favor.
    For,
    /// Vote against.
    Against,
    /// Abstain from the vote.
    Abstain,
}

/// Reward source contract reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// Source contract address, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Human-readable source label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// EIP-712 domain envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignMessageDomain {
    /// Domain name, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Domain version, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Domain chain id, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<u64>,
    /// Verifying contract, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifying_contract: Option<Address>,
    /// Domain salt, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub salt: Option<Hex>,
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

    pub(crate) fn source() -> Value {
        json!({
            "address": address(0x20),
            "label": "Rewards Source"
        })
    }

    pub(crate) fn validity(source: &str) -> Value {
        json!({
            "expiresAt": "1700000000",
            "source": source
        })
    }

    pub(crate) fn domain() -> Value {
        json!({
            "name": "Example App",
            "version": "1",
            "verifyingContract": address(0x21),
            "salt": hex32(0x22)
        })
    }
}
