//! `RequestWithdrawalAction` — enter the liquid-staking withdrawal queue.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::StakingVenue;

/// Request a withdrawal — burns the staking token and mints withdrawal-request
/// NFTs to `owner`, entering the protocol's withdrawal queue.
///
/// Models Lido `requestWithdrawals(uint256[] _amounts, address _owner)`, the
/// `…WstETH` variant, and the `…WithPermit` variants (the embedded EIP-2612
/// permit is a standard allowance grant, not represented here). `token`
/// distinguishes stETH vs wstETH; one withdrawal NFT is minted per `amounts` element.
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
}
