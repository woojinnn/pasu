//! Action envelope and action category types.

use serde::{Deserialize, Serialize};

/// High-level category assigned to an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Decentralized exchange activity.
    Dex,
    /// Lending market activity.
    Lending,
    /// Real-world asset activity.
    Rwa,
    /// Liquid staking activity.
    LiquidStaking,
    /// Restaking activity.
    Restaking,
    /// Yield strategy activity.
    Yield,
    /// Miscellaneous activity.
    Misc,
    /// Unknown category.
    Unknown,
}

/// Protocol-normalized action payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "fields", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum Action {
    /// Swap tokens through a DEX.
    Swap(crate::action::dex::SwapAction),
    /// Add liquidity to a DEX pool.
    AddLiquidity(crate::action::dex::AddLiquidityAction),
    /// Remove liquidity from a DEX pool.
    RemoveLiquidity(crate::action::dex::RemoveLiquidityAction),
    /// Mint a concentrated-liquidity NFT position.
    MintLiquidityNft(crate::action::dex::MintLiquidityNftAction),
    /// Burn a concentrated-liquidity NFT position.
    BurnLiquidityNft(crate::action::dex::BurnLiquidityNftAction),
    /// Increase liquidity in an NFT position.
    IncreaseLiquidity(crate::action::dex::IncreaseLiquidityAction),
    /// Decrease liquidity in an NFT position.
    DecreaseLiquidity(crate::action::dex::DecreaseLiquidityAction),
    /// Push assets into a V4 pool's in-range LPs without minting.
    Donate(crate::action::dex::DonateAction),
    /// Create a new pool or set its initial price.
    InitializePool(crate::action::dex::InitializePoolAction),
    /// Supply assets to a lending market.
    Supply(crate::action::lending::SupplyAction),
    /// Withdraw assets from a lending market.
    Withdraw(crate::action::lending::WithdrawAction),
    /// Borrow assets from a lending market.
    Borrow(crate::action::lending::BorrowAction),
    /// Repay lending market debt.
    Repay(crate::action::lending::RepayAction),
    /// Liquidate an unhealthy lending position.
    Liquidate(crate::action::lending::LiquidateAction),
    /// Borrow and repay assets in one transaction.
    FlashLoan(crate::action::lending::FlashLoanAction),
    /// Set on-chain lending authorization.
    SetAuthorization(crate::action::lending::SetAuthorizationAction),
    /// Sign a lending authorization payload.
    SignAuthorization(crate::action::lending::SignAuthorizationAction),
    /// Revoke lending authority.
    Revoke(crate::action::lending::RevokeAction),
    /// Wrap a native asset.
    Wrap(crate::action::misc::WrapAction),
    /// Unwrap a wrapped native asset.
    Unwrap(crate::action::misc::UnwrapAction),
    /// Approve an amount-based token allowance.
    Approve(crate::action::misc::ApproveAction),
    /// Toggle collection-wide operator approval.
    SetApprovalForAll(crate::action::misc::SetApprovalForAllAction),
    /// Transfer a token.
    Transfer(crate::action::misc::TransferAction),
    /// Sign or relay a token permit.
    Permit(crate::action::misc::PermitAction),
    /// Claim accrued rewards.
    ClaimRewards(crate::action::misc::ClaimRewardsAction),
    /// Sign an unnormalized EIP-712 message.
    SignMessage(crate::action::misc::SignMessageAction),
    /// Delegate governance power.
    Delegate(crate::action::misc::DelegateAction),
    /// Cast a governance vote.
    Vote(crate::action::misc::VoteAction),
    /// Stake an asset.
    Stake(crate::action::staking::StakeAction),
    /// Request delayed unstaking.
    RequestUnstake(crate::action::staking::RequestUnstakeAction),
    /// Claim a completed unstake.
    ClaimUnstake(crate::action::staking::ClaimUnstakeAction),
    /// Restake an asset.
    Restake(crate::action::restaking::RestakeAction),
    /// Request delayed restaking withdrawal.
    RequestRestakeWithdrawal(crate::action::restaking::RequestRestakeWithdrawalAction),
    /// Claim a completed restaking withdrawal.
    ClaimRestakeWithdrawal(crate::action::restaking::ClaimRestakeWithdrawalAction),
}

impl Action {
    /// Returns the canonical `snake_case` action kind.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Swap(_) => "swap",
            Self::AddLiquidity(_) => "add_liquidity",
            Self::RemoveLiquidity(_) => "remove_liquidity",
            Self::MintLiquidityNft(_) => "mint_liquidity_nft",
            Self::BurnLiquidityNft(_) => "burn_liquidity_nft",
            Self::IncreaseLiquidity(_) => "increase_liquidity",
            Self::DecreaseLiquidity(_) => "decrease_liquidity",
            Self::Donate(_) => "donate",
            Self::InitializePool(_) => "initialize_pool",
            Self::Supply(_) => "supply",
            Self::Withdraw(_) => "withdraw",
            Self::Borrow(_) => "borrow",
            Self::Repay(_) => "repay",
            Self::Liquidate(_) => "liquidate",
            Self::FlashLoan(_) => "flash_loan",
            Self::SetAuthorization(_) => "set_authorization",
            Self::SignAuthorization(_) => "sign_authorization",
            Self::Revoke(_) => "revoke",
            Self::Wrap(_) => "wrap",
            Self::Unwrap(_) => "unwrap",
            Self::Approve(_) => "approve",
            Self::SetApprovalForAll(_) => "set_approval_for_all",
            Self::Transfer(_) => "transfer",
            Self::Permit(_) => "permit",
            Self::ClaimRewards(_) => "claim_rewards",
            Self::SignMessage(_) => "sign_message",
            Self::Delegate(_) => "delegate",
            Self::Vote(_) => "vote",
            Self::Stake(_) => "stake",
            Self::RequestUnstake(_) => "request_unstake",
            Self::ClaimUnstake(_) => "claim_unstake",
            Self::Restake(_) => "restake",
            Self::RequestRestakeWithdrawal(_) => "request_restake_withdrawal",
            Self::ClaimRestakeWithdrawal(_) => "claim_restake_withdrawal",
        }
    }

    /// Returns the default high-level category for this action kind.
    #[must_use]
    pub const fn default_category(&self) -> Category {
        match self {
            Self::Swap(_)
            | Self::AddLiquidity(_)
            | Self::RemoveLiquidity(_)
            | Self::MintLiquidityNft(_)
            | Self::BurnLiquidityNft(_)
            | Self::IncreaseLiquidity(_)
            | Self::DecreaseLiquidity(_)
            | Self::Donate(_)
            | Self::InitializePool(_) => Category::Dex,
            Self::Supply(_)
            | Self::Withdraw(_)
            | Self::Borrow(_)
            | Self::Repay(_)
            | Self::Liquidate(_)
            | Self::FlashLoan(_)
            | Self::SetAuthorization(_)
            | Self::SignAuthorization(_)
            | Self::Revoke(_) => Category::Lending,
            Self::Stake(_) | Self::RequestUnstake(_) | Self::ClaimUnstake(_) => {
                Category::LiquidStaking
            }
            Self::Restake(_)
            | Self::RequestRestakeWithdrawal(_)
            | Self::ClaimRestakeWithdrawal(_) => Category::Restaking,
            Self::Wrap(_)
            | Self::Unwrap(_)
            | Self::Approve(_)
            | Self::SetApprovalForAll(_)
            | Self::Transfer(_)
            | Self::Permit(_)
            | Self::ClaimRewards(_)
            | Self::SignMessage(_)
            | Self::Delegate(_)
            | Self::Vote(_) => Category::Misc,
        }
    }
}

/// Categorized action envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    /// High-level category.
    pub category: Category,
    /// Tagged action payload.
    #[serde(flatten)]
    pub action: Action,
}

#[cfg(test)]
mod tests {
    use super::{Action, ActionEnvelope, Category};
    use serde_json::{json, Value};

    #[test]
    fn test_category_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&Category::LiquidStaking).unwrap(),
            r#""liquid_staking""#
        );
        assert_eq!(
            serde_json::from_str::<Category>(r#""restaking""#).unwrap(),
            Category::Restaking
        );
    }

    #[test]
    fn test_action_kind_for_each_variant() {
        for (action, expected_kind, _) in sample_actions() {
            assert_eq!(action.kind(), expected_kind);
        }
    }

    #[test]
    fn test_action_default_category() {
        for (action, _, expected_category) in sample_actions() {
            assert_eq!(action.default_category(), expected_category);
        }
    }

    #[test]
    fn test_action_envelope_swap_wire_format() {
        let swap = serde_json::from_value(swap_fields()).unwrap();
        let envelope = ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(swap),
        };

        let value = serde_json::to_value(&envelope).unwrap();

        assert_eq!(value.get("category"), Some(&json!("dex")));
        assert_eq!(value.get("action"), Some(&json!("swap")));
        assert_eq!(value.get("fields"), Some(&swap_fields()));
        assert_eq!(
            serde_json::from_value::<ActionEnvelope>(value).unwrap(),
            envelope
        );
    }

    #[test]
    fn test_action_envelope_approve_wire_format() {
        let approve = serde_json::from_value(approve_fields()).unwrap();
        let envelope = ActionEnvelope {
            category: Category::Misc,
            action: Action::Approve(approve),
        };

        let value = serde_json::to_value(&envelope).unwrap();

        assert_eq!(value.get("category"), Some(&json!("misc")));
        assert_eq!(value.get("action"), Some(&json!("approve")));
        assert_eq!(value.get("fields"), Some(&approve_fields()));
        assert_eq!(
            serde_json::from_value::<ActionEnvelope>(value).unwrap(),
            envelope
        );
    }

    #[allow(clippy::too_many_lines)]
    fn sample_actions() -> Vec<(Action, &'static str, Category)> {
        vec![
            (action("swap", swap_fields()), "swap", Category::Dex),
            (
                action("add_liquidity", add_liquidity_fields()),
                "add_liquidity",
                Category::Dex,
            ),
            (
                action("remove_liquidity", remove_liquidity_fields()),
                "remove_liquidity",
                Category::Dex,
            ),
            (
                action("mint_liquidity_nft", mint_liquidity_nft_fields()),
                "mint_liquidity_nft",
                Category::Dex,
            ),
            (
                action("burn_liquidity_nft", burn_liquidity_nft_fields()),
                "burn_liquidity_nft",
                Category::Dex,
            ),
            (
                action("increase_liquidity", increase_liquidity_fields()),
                "increase_liquidity",
                Category::Dex,
            ),
            (
                action("decrease_liquidity", decrease_liquidity_fields()),
                "decrease_liquidity",
                Category::Dex,
            ),
            (action("donate", donate_fields()), "donate", Category::Dex),
            (
                action("initialize_pool", initialize_pool_fields()),
                "initialize_pool",
                Category::Dex,
            ),
            (
                action("supply", supply_fields()),
                "supply",
                Category::Lending,
            ),
            (
                action("withdraw", withdraw_fields()),
                "withdraw",
                Category::Lending,
            ),
            (
                action("borrow", borrow_fields()),
                "borrow",
                Category::Lending,
            ),
            (action("repay", repay_fields()), "repay", Category::Lending),
            (
                action("liquidate", liquidate_fields()),
                "liquidate",
                Category::Lending,
            ),
            (
                action("flash_loan", flash_loan_fields()),
                "flash_loan",
                Category::Lending,
            ),
            (
                action("set_authorization", set_authorization_fields()),
                "set_authorization",
                Category::Lending,
            ),
            (
                action("sign_authorization", sign_authorization_fields()),
                "sign_authorization",
                Category::Lending,
            ),
            (
                action("revoke", revoke_fields()),
                "revoke",
                Category::Lending,
            ),
            (action("wrap", wrap_fields()), "wrap", Category::Misc),
            (action("unwrap", unwrap_fields()), "unwrap", Category::Misc),
            (
                action("approve", approve_fields()),
                "approve",
                Category::Misc,
            ),
            (
                action("set_approval_for_all", set_approval_for_all_fields()),
                "set_approval_for_all",
                Category::Misc,
            ),
            (
                action("transfer", transfer_fields()),
                "transfer",
                Category::Misc,
            ),
            (action("permit", permit_fields()), "permit", Category::Misc),
            (
                action("claim_rewards", claim_rewards_fields()),
                "claim_rewards",
                Category::Misc,
            ),
            (
                action("sign_message", sign_message_fields()),
                "sign_message",
                Category::Misc,
            ),
            (
                action("delegate", delegate_fields()),
                "delegate",
                Category::Misc,
            ),
            (action("vote", vote_fields()), "vote", Category::Misc),
            (
                action("stake", stake_fields()),
                "stake",
                Category::LiquidStaking,
            ),
            (
                action("request_unstake", request_unstake_fields()),
                "request_unstake",
                Category::LiquidStaking,
            ),
            (
                action("claim_unstake", claim_unstake_fields()),
                "claim_unstake",
                Category::LiquidStaking,
            ),
            (
                action("restake", restake_fields()),
                "restake",
                Category::Restaking,
            ),
            (
                action(
                    "request_restake_withdrawal",
                    request_restake_withdrawal_fields(),
                ),
                "request_restake_withdrawal",
                Category::Restaking,
            ),
            (
                action(
                    "claim_restake_withdrawal",
                    claim_restake_withdrawal_fields(),
                ),
                "claim_restake_withdrawal",
                Category::Restaking,
            ),
        ]
    }

    #[allow(clippy::needless_pass_by_value)]
    fn action(action: &str, fields: Value) -> Action {
        serde_json::from_value(json!({ "action": action, "fields": fields })).unwrap()
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
            "tokenId": "1",
            "symbol": symbol
        })
    }

    fn amount(kind: &str, value: &str) -> Value {
        json!({ "kind": kind, "value": value })
    }

    fn validity() -> Value {
        json!({
            "expiresAt": "1700000000",
            "source": "signature-deadline"
        })
    }

    fn asset_amount_pair(first_kind: &str, second_kind: &str) -> Value {
        json!([
            {
                "asset": erc20("WETH"),
                "amount": amount(first_kind, "1000")
            },
            {
                "asset": erc20("USDC"),
                "amount": amount(second_kind, "900")
            }
        ])
    }

    #[allow(clippy::needless_pass_by_value)]
    fn asset_amount(asset: Value, kind: &str, value: &str) -> Value {
        json!({
            "asset": asset,
            "amount": amount(kind, value)
        })
    }

    fn pool() -> Value {
        json!({
            "address": address(0x20),
            "id": hex32(0x21),
            "label": "ETH/USDC 0.05%"
        })
    }

    fn erc721_instance(symbol: &str, token_id: &str) -> Value {
        json!({
            "kind": "erc721",
            "address": address(0x11),
            "tokenId": token_id,
            "symbol": symbol
        })
    }

    fn swap_fields() -> Value {
        json!({
            "swapMode": "exact_in",
            "inputToken": asset_amount(erc20("WETH"), "exact", "1000"),
            "outputToken": asset_amount(erc20("USDC"), "min", "900"),
            "recipient": address(0x30)
        })
    }

    fn add_liquidity_fields() -> Value {
        json!({
            "pool": pool(),
            "inputTokens": asset_amount_pair("exact", "exact"),
            "outputLp": asset_amount(erc20("UNI-V2"), "min", "100"),
            "recipient": address(0x30)
        })
    }

    fn remove_liquidity_fields() -> Value {
        json!({
            "exitMode": "proportional",
            "pool": pool(),
            "inputLp": asset_amount(erc20("UNI-V2"), "exact", "100"),
            "outputTokens": asset_amount_pair("min", "min"),
            "recipient": address(0x30)
        })
    }

    fn mint_liquidity_nft_fields() -> Value {
        json!({
            "pool": pool(),
            "feeBps": 5,
            "tickRange": {
                "lower": -60,
                "upper": 60
            },
            "inputTokens": asset_amount_pair("min", "min"),
            "recipient": address(0x30)
        })
    }

    fn burn_liquidity_nft_fields() -> Value {
        json!({
            "nft": erc721_instance("UNI-V3-POS", "42"),
            "burnKind": "empty_only"
        })
    }

    fn increase_liquidity_fields() -> Value {
        json!({
            "nft": erc721_instance("UNI-V3-POS", "42"),
            "inputTokens": asset_amount_pair("min", "min")
        })
    }

    fn decrease_liquidity_fields() -> Value {
        json!({
            "nft": erc721_instance("UNI-V3-POS", "42"),
            "liquidityDelta": amount("exact", "1000"),
            "outputTokens": asset_amount_pair("min", "min")
        })
    }

    fn donate_fields() -> Value {
        json!({
            "pool": pool(),
            "inputTokens": asset_amount_pair("exact", "exact")
        })
    }

    fn initialize_pool_fields() -> Value {
        json!({
            "pool": pool(),
            "token0": erc20("WETH"),
            "token1": erc20("USDC"),
            "feeBps": 500
        })
    }

    fn supply_fields() -> Value {
        json!({
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    }

    fn withdraw_fields() -> Value {
        supply_fields()
    }

    fn borrow_fields() -> Value {
        json!({
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30),
            "onBehalf": address(0x31)
        })
    }

    fn repay_fields() -> Value {
        json!({
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "onBehalf": address(0x31),
            "repayKind": "debt_asset"
        })
    }

    fn liquidate_fields() -> Value {
        json!({
            "borrower": address(0x40),
            "debtAsset": erc20("USDC"),
            "liquidationKind": "pool_share"
        })
    }

    fn flash_loan_fields() -> Value {
        json!({
            "assets": [erc20("USDC")],
            "amounts": [amount("exact", "1000")],
            "receiver": address(0x50),
            "flashLoanKind": "simple"
        })
    }

    fn set_authorization_fields() -> Value {
        json!({
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": true,
            "authorizationScope": "all"
        })
    }

    fn sign_authorization_fields() -> Value {
        json!({
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": true,
            "authorizationScope": "all",
            "validity": validity()
        })
    }

    fn revoke_fields() -> Value {
        json!({
            "caller": address(0x70),
            "subject": address(0x71),
            "revokeKind": "erc20_allowance"
        })
    }

    fn wrap_fields() -> Value {
        json!({
            "nativeAsset": {
                "asset": native("ETH"),
                "amount": amount("exact", "1000")
            },
            "wrappedAsset": {
                "asset": erc20("WETH"),
                "amount": amount("exact", "1000")
            },
            "recipient": address(0x30)
        })
    }

    fn unwrap_fields() -> Value {
        json!({
            "wrappedAsset": {
                "asset": erc20("WETH"),
                "amount": amount("exact", "1000")
            },
            "nativeAsset": {
                "asset": native("ETH"),
                "amount": amount("exact", "1000")
            },
            "recipient": address(0x30)
        })
    }

    fn approve_fields() -> Value {
        json!({
            "token": erc20("USDC"),
            "spender": address(0x40),
            "amount": amount("exact", "1000"),
            "approvalKind": "erc20"
        })
    }

    fn set_approval_for_all_fields() -> Value {
        json!({
            "collection": erc721("NFT"),
            "operator": address(0x41),
            "approved": true
        })
    }

    fn transfer_fields() -> Value {
        json!({
            "token": {
                "asset": erc20("USDC"),
                "amount": amount("exact", "1000")
            },
            "from": address(0x50),
            "recipient": address(0x51)
        })
    }

    fn permit_fields() -> Value {
        json!({
            "permitKind": "eip2612",
            "token": erc20("USDC"),
            "owner": address(0x52),
            "spender": address(0x53),
            "amount": amount("exact", "1000"),
            "validity": validity()
        })
    }

    fn claim_rewards_fields() -> Value {
        json!({
            "from": address(0x60),
            "recipient": address(0x61)
        })
    }

    fn sign_message_fields() -> Value {
        json!({
            "domain": {},
            "primaryType": "Order",
            "messageDigest": hex32(0x70)
        })
    }

    fn delegate_fields() -> Value {
        json!({
            "token": erc20("GOV"),
            "delegatee": address(0x80)
        })
    }

    fn vote_fields() -> Value {
        json!({
            "governance": address(0x90),
            "proposalId": "1",
            "support": "for"
        })
    }

    fn stake_fields() -> Value {
        json!({
            "tokenIn": native("ETH"),
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    }

    fn request_unstake_fields() -> Value {
        json!({
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    }

    fn claim_unstake_fields() -> Value {
        json!({
            "tokenOut": native("ETH"),
            "ticket": {},
            "recipient": address(0x30)
        })
    }

    fn restake_fields() -> Value {
        json!({
            "tokenIn": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    }

    fn request_restake_withdrawal_fields() -> Value {
        json!({
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    }

    fn claim_restake_withdrawal_fields() -> Value {
        json!({
            "tokenOut": native("ETH"),
            "ticket": {},
            "recipient": address(0x30)
        })
    }
}
