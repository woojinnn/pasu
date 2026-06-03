//! `PeripheryOperationAction` — high-risk Aave periphery adapter calls.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::LendingVenue;

/// Aave periphery adapters bundle swaps, flash liquidity, collateral movement,
/// and lending calls. The wallet-visible intent is preserved for policy, while
/// reducer-side balance simulation stays fail-closed/no-op.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct PeripheryOperationAction {
    /// Aave periphery venue.
    pub venue: LendingVenue,
    /// Adapter contract the user directly calls.
    #[tsify(type = "string")]
    pub adapter: Address,
    /// Periphery operation category.
    pub kind: LendingPeripheryKind,
    /// Primary input asset, when statically decodable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub asset_in: Option<TokenRef>,
    /// Primary output/debt/collateral asset, when statically decodable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub asset_out: Option<TokenRef>,
    /// Primary amount controlled by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub amount: Option<U256>,
    /// Limit amount such as min-out or max-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub limit_amount: Option<U256>,
    /// User/borrower whose position is affected, when explicit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub user: Option<Address>,
    /// Recipient of any output tokens, when explicit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub recipient: Option<Address>,
    /// Full calldata retained for audit/replay because adapter callbacks can
    /// encode protocol-specific paths and nested execution.
    #[tsify(type = "string")]
    pub calldata: String,
}

/// Coarse Aave periphery operation category.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum LendingPeripheryKind {
    /// Swap supplied collateral to another reserve.
    SwapCollateral,
    /// Repay debt by selling collateral.
    RepayWithCollateral,
    /// Swap one debt exposure to another debt asset.
    DebtSwap,
    /// Migrate positions between Aave deployments.
    Migration,
    /// Withdraw collateral and swap to another asset.
    WithdrawSwap,
    /// Selector belongs to a user-facing periphery adapter but lacks a stable
    /// semantic subtype.
    Raw,
}
