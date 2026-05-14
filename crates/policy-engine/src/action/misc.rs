//! Miscellaneous action schema types.

use serde::{Deserialize, Serialize};

use super::common::{Address, AmountConstraint, AssetRef, DecimalString, Hex, Validity};

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

/// Wrap a native asset into its ERC-20 representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrapAction {
    /// Native asset being wrapped.
    pub native_asset: AssetRef,
    /// Wrapped ERC-20 asset being minted.
    pub wrapped_asset: AssetRef,
    /// Wrap amount.
    pub amount: AmountConstraint,
    /// Recipient of the wrapped asset.
    pub recipient: Address,
}

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

/// Approve a spender for an amount-based token allowance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveAction {
    /// Token being approved.
    pub token: AssetRef,
    /// Spender receiving allowance.
    pub spender: Address,
    /// Human-readable spender label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spender_label: Option<String>,
    /// Approved amount.
    pub amount: AmountConstraint,
    /// Approval variant.
    pub approval_kind: ApprovalKind,
    /// Current allowance before this action, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_allowance: Option<DecimalString>,
    /// Approval validity window, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

/// Toggle collection-wide NFT operator approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetApprovalForAllAction {
    /// NFT collection whose operator approval changes.
    pub collection: AssetRef,
    /// Operator receiving or losing approval.
    pub operator: Address,
    /// Human-readable operator label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_label: Option<String>,
    /// Whether collection-wide approval is granted.
    pub approved: bool,
    /// Previous approval state, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previously_approved: Option<bool>,
}

/// Transfer a token directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferAction {
    /// Token being transferred.
    pub token: AssetRef,
    /// Account sending the token.
    pub from: Address,
    /// Account receiving the token.
    pub recipient: Address,
    /// Fungible or ERC-1155 amount, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountConstraint>,
    /// NFT or ERC-1155 token id, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
}

/// Sign or relay a token permit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermitAction {
    /// Permit variant.
    pub permit_kind: PermitKind,
    /// Token authorized by the permit.
    pub token: AssetRef,
    /// Permit owner and signer.
    pub owner: Address,
    /// Spender for allowance-style permits, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spender: Option<Address>,
    /// Human-readable spender label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spender_label: Option<String>,
    /// Recipient for transfer-style permits, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Permitted amount or amount cap.
    pub amount: AmountConstraint,
    /// Requested transfer amount, when distinct from the cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_amount: Option<AmountConstraint>,
    /// Primary permit validity window.
    pub validity: Validity,
    /// Signature relay validity window, when separate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_validity: Option<Validity>,
}

/// Claim accrued reward tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRewardsAction {
    /// Reward source contract, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
    /// Position NFT collection, when rewards are NFT-position based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nft: Option<AssetRef>,
    /// Position NFT token id, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
    /// Account whose rewards are claimed.
    pub from: Address,
    /// Account receiving claimed rewards.
    pub recipient: Address,
    /// Reward token list, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_tokens: Option<Vec<AssetRef>>,
    /// Maximum claim amounts matching `reward_tokens`, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_amounts: Option<Vec<AmountConstraint>>,
}

/// Sign an EIP-712 message envelope that was not normalized further.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignMessageAction {
    /// EIP-712 domain.
    pub domain: SignMessageDomain,
    /// EIP-712 primary type.
    pub primary_type: String,
    /// Human-readable domain label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_label: Option<String>,
    /// Human-readable primary type label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_type_label: Option<String>,
    /// EIP-712 message digest.
    pub message_digest: Hex,
}

/// Delegate governance voting power.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateAction {
    /// Governance token whose power is delegated.
    pub token: AssetRef,
    /// Delegate receiving voting power.
    pub delegatee: Address,
    /// Human-readable delegate label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegatee_label: Option<String>,
    /// Current delegate before this action, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_delegate: Option<Address>,
    /// Voting power affected by this action, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voting_power: Option<DecimalString>,
    /// Power type for split-power governance tokens, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_type: Option<PowerType>,
    /// Signature validity window, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

/// Cast a governance vote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteAction {
    /// Governor contract.
    pub governance: Address,
    /// Human-readable governance label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governance_label: Option<String>,
    /// Proposal identifier.
    pub proposal_id: DecimalString,
    /// Vote direction.
    pub support: VoteSupport,
    /// Free-form vote reason, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Voting power applied to this vote, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voting_power: Option<DecimalString>,
    /// Signature validity window, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};
    use std::fmt::Debug;

    #[allow(clippy::needless_pass_by_value)]
    fn assert_json_roundtrip<T>(fixture: Value)
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let action = serde_json::from_value::<T>(fixture.clone()).unwrap();
        let serialized = serde_json::to_value(action).unwrap();

        assert_eq!(serialized, fixture);
    }

    fn address(value: u8) -> String {
        format!("0x{value:040x}")
    }

    fn hex32(value: u8) -> String {
        format!("0x{}", format!("{value:02x}").repeat(32))
    }

    fn native(symbol: &str) -> Value {
        json!({
            "kind": "native",
            "symbol": symbol,
            "decimals": 18
        })
    }

    fn erc20(symbol: &str) -> Value {
        json!({
            "kind": "erc20",
            "address": address(0x10),
            "symbol": symbol,
            "decimals": 18
        })
    }

    fn erc721(symbol: &str) -> Value {
        json!({
            "kind": "erc721",
            "address": address(0x11),
            "symbol": symbol
        })
    }

    fn amount(kind: &str, value: &str) -> Value {
        json!({ "kind": kind, "value": value })
    }

    fn source() -> Value {
        json!({
            "address": address(0x20),
            "label": "Rewards Source"
        })
    }

    fn validity(source: &str) -> Value {
        json!({
            "expiresAt": "1700000000",
            "source": source
        })
    }

    fn domain() -> Value {
        json!({
            "name": "Example App",
            "version": "1",
            "verifyingContract": address(0x21),
            "salt": hex32(0x22)
        })
    }

    macro_rules! roundtrip_test {
        ($name:ident, $ty:ty, $fixture:expr) => {
            #[test]
            fn $name() {
                assert_json_roundtrip::<$ty>($fixture);
            }
        };
    }

    roundtrip_test!(
        test_wrap_action_serde_roundtrip_minimal,
        WrapAction,
        json!({
            "nativeAsset": native("ETH"),
            "wrappedAsset": erc20("WETH"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_wrap_action_serde_roundtrip_full,
        WrapAction,
        json!({
            "nativeAsset": native("ETH"),
            "wrappedAsset": erc20("WETH"),
            "amount": amount("exact", "2000"),
            "recipient": address(0x31)
        })
    );

    roundtrip_test!(
        test_unwrap_action_serde_roundtrip_minimal,
        UnwrapAction,
        json!({
            "wrappedAsset": erc20("WETH"),
            "nativeAsset": native("ETH"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_unwrap_action_serde_roundtrip_full,
        UnwrapAction,
        json!({
            "wrappedAsset": erc20("WETH"),
            "nativeAsset": native("ETH"),
            "amount": amount("min", "900"),
            "recipient": address(0x31)
        })
    );

    roundtrip_test!(
        test_approve_action_serde_roundtrip_minimal,
        ApproveAction,
        json!({
            "token": erc20("USDC"),
            "spender": address(0x40),
            "amount": amount("exact", "1000"),
            "approvalKind": "erc20"
        })
    );

    roundtrip_test!(
        test_approve_action_serde_roundtrip_full,
        ApproveAction,
        json!({
            "token": erc20("USDC"),
            "spender": address(0x40),
            "spenderLabel": "Known Router",
            "amount": amount("unlimited", "0"),
            "approvalKind": "permit2",
            "currentAllowance": "500",
            "validity": validity("grant-expiration")
        })
    );

    roundtrip_test!(
        test_set_approval_for_all_action_serde_roundtrip_minimal,
        SetApprovalForAllAction,
        json!({
            "collection": erc721("NFT"),
            "operator": address(0x41),
            "approved": true
        })
    );

    roundtrip_test!(
        test_set_approval_for_all_action_serde_roundtrip_full,
        SetApprovalForAllAction,
        json!({
            "collection": erc721("NFT"),
            "operator": address(0x41),
            "operatorLabel": "Known Operator",
            "approved": false,
            "previouslyApproved": true
        })
    );

    roundtrip_test!(
        test_transfer_action_serde_roundtrip_minimal,
        TransferAction,
        json!({
            "token": erc20("USDC"),
            "from": address(0x50),
            "recipient": address(0x51)
        })
    );

    roundtrip_test!(
        test_transfer_action_serde_roundtrip_full,
        TransferAction,
        json!({
            "token": erc721("NFT"),
            "from": address(0x50),
            "recipient": address(0x51),
            "amount": amount("exact", "1"),
            "tokenId": "42"
        })
    );

    roundtrip_test!(
        test_permit_action_serde_roundtrip_minimal,
        PermitAction,
        json!({
            "permitKind": "eip2612",
            "token": erc20("USDC"),
            "owner": address(0x52),
            "amount": amount("exact", "1000"),
            "validity": validity("signature-deadline")
        })
    );

    roundtrip_test!(
        test_permit_action_serde_roundtrip_full,
        PermitAction,
        json!({
            "permitKind": "permit2_transfer",
            "token": erc20("USDC"),
            "owner": address(0x52),
            "spender": address(0x53),
            "spenderLabel": "Known Spender",
            "recipient": address(0x54),
            "amount": amount("max", "1000"),
            "requestedAmount": amount("exact", "900"),
            "validity": validity("signature-deadline"),
            "signatureValidity": validity("signature-deadline")
        })
    );

    roundtrip_test!(
        test_claim_rewards_action_serde_roundtrip_minimal,
        ClaimRewardsAction,
        json!({
            "from": address(0x60),
            "recipient": address(0x61)
        })
    );

    roundtrip_test!(
        test_claim_rewards_action_serde_roundtrip_full,
        ClaimRewardsAction,
        json!({
            "source": source(),
            "nft": erc721("POSITION"),
            "tokenId": "42",
            "from": address(0x60),
            "recipient": address(0x61),
            "rewardTokens": [erc20("USDC"), erc20("WETH")],
            "maxAmounts": [amount("max", "1000"), amount("max", "2")]
        })
    );

    roundtrip_test!(
        test_sign_message_action_serde_roundtrip_minimal,
        SignMessageAction,
        json!({
            "domain": {},
            "primaryType": "Order",
            "messageDigest": hex32(0x70)
        })
    );

    roundtrip_test!(
        test_sign_message_action_serde_roundtrip_full,
        SignMessageAction,
        json!({
            "domain": domain(),
            "primaryType": "Order",
            "domainLabel": "Example App",
            "primaryTypeLabel": "Order Signature",
            "messageDigest": hex32(0x70)
        })
    );

    roundtrip_test!(
        test_delegate_action_serde_roundtrip_minimal,
        DelegateAction,
        json!({
            "token": erc20("GOV"),
            "delegatee": address(0x80)
        })
    );

    roundtrip_test!(
        test_delegate_action_serde_roundtrip_full,
        DelegateAction,
        json!({
            "token": erc20("GOV"),
            "delegatee": address(0x80),
            "delegateeLabel": "Known Delegate",
            "currentDelegate": address(0x81),
            "votingPower": "1000000",
            "powerType": "proposition",
            "validity": validity("signature-deadline")
        })
    );

    roundtrip_test!(
        test_vote_action_serde_roundtrip_minimal,
        VoteAction,
        json!({
            "governance": address(0x90),
            "proposalId": "1",
            "support": "for"
        })
    );

    roundtrip_test!(
        test_vote_action_serde_roundtrip_full,
        VoteAction,
        json!({
            "governance": address(0x90),
            "governanceLabel": "Example Governor",
            "proposalId": "1",
            "support": "abstain",
            "reason": "reason",
            "votingPower": "1000000",
            "validity": validity("signature-deadline")
        })
    );
}
