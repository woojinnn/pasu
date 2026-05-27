//! Miscellaneous action schema types.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, Hex};

mod approve;
mod claim_rewards;
mod delegate;
mod gauge_vote;
mod lock_create;
mod lock_increase;
mod lock_manage;
mod lock_withdraw;
mod lp_stake;
mod lp_unstake;
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
pub use gauge_vote::{GaugeVoteAction, GaugeVoteKind};
pub use lock_create::LockCreateAction;
pub use lock_increase::{LockIncreaseAction, LockIncreaseKind};
pub use lock_manage::{LockManageAction, LockManageKind};
pub use lock_withdraw::LockWithdrawAction;
pub use lp_stake::LpStakeAction;
pub use lp_unstake::LpUnstakeAction;
pub use permit::PermitAction;
pub use set_approval_for_all::SetApprovalForAllAction;
pub use sign_message::SignMessageAction;
pub use transfer::TransferAction;
pub use unwrap::UnwrapAction;
pub use vote::VoteAction;
pub use wrap::WrapAction;

/// ERC-20/721 approval variant.
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
    /// ERC-721 single-token approval (NFT).
    Erc721,
}

/// Permit signature variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermitKind {
    /// EIP-2612 token permit.
    Eip2612,
    /// ERC-721 token permit.
    Erc721Permit,
    /// ERC-721 approval-for-all permit.
    Erc721PermitForAll,
    /// Permit2 allowance grant.
    Permit2Single,
    /// Permit2 one-shot transfer authorization.
    Permit2Transfer,
    /// Permit2 batched allowance grant (`PermitBatch` — multi-token in a single
    /// signature). Carries the same risk profile as `Permit2Single`
    /// (allowance + spender + signature deadline) but applies across a list
    /// of tokens. In the `PoC` the mapper layer collapses the batch down to
    /// `details[0]` so the schema can keep a single `token` slot; full
    /// fan-out is a follow-up.
    Permit2Batch,
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
    use serde_json::{json, Value};

    pub(crate) use crate::action::test_support::{
        address, amount, assert_json_roundtrip, erc20, erc721, hex32, native,
    };

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
