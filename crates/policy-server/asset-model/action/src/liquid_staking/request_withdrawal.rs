//! `RequestWithdrawalAction` — enter the liquid-staking withdrawal queue.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::StakingVenue;

/// An EIP-2612 permit embedded in the calldata of a `*WithPermit` withdrawal
/// request. The permit grants the WithdrawalQueue — the implicit spender (the
/// `to` contract itself) — an allowance over the user's stETH/wstETH so the queue
/// can pull the tokens in the same tx. Only the policy-relevant fields are
/// modeled; the signature (`v`, `r`, `s`) is decode-only and intentionally dropped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct EmbeddedPermit {
    /// Allowance granted to the queue (EIP-2612 `value`, token base units).
    #[tsify(type = "string")]
    pub value: U256,
    /// Permit expiry (EIP-2612 `deadline`, unix seconds).
    #[tsify(type = "string")]
    pub deadline: U256,
}

/// Request a withdrawal — burns the staking token and mints withdrawal-request
/// NFTs to `owner`, entering the protocol's withdrawal queue.
///
/// Models Lido `requestWithdrawals(uint256[] _amounts, address _owner)`, the
/// `…WstETH` variant, and the `…WithPermit` variants. For the `…WithPermit`
/// variants the in-calldata EIP-2612 permit is surfaced as `embedded_permit` (the
/// allowance grant to the queue); it is absent for the non-permit variants.
/// `token` distinguishes stETH vs wstETH; one withdrawal NFT is minted per
/// `amounts` element.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RequestWithdrawalAction {
    /// Liquid-staking venue.
    pub venue: StakingVenue,
    /// Token being queued for withdrawal (stETH or wstETH).
    pub token: TokenRef,
    /// Per-request amounts (one withdrawal-request NFT per element).
    #[tsify(type = "string[]")]
    pub amounts: Vec<U256>,
    /// Owner of the minted withdrawal-request NFTs.
    #[tsify(type = "string")]
    pub owner: Address,
    /// EIP-2612 permit embedded in the `…WithPermit` variants (absent otherwise).
    /// The spender is implicit — the WithdrawalQueue (`to`) itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded_permit: Option<EmbeddedPermit>,
}
