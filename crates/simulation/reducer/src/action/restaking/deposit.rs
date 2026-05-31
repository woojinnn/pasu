//! `DepositAction` — deposit an ERC-20 (LST) into a strategy for restaking shares.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::RestakingVenue;

/// Deposit into a strategy. Models StrategyManager
/// `depositIntoStrategy(address strategy, address token, uint256 amount)` and
/// the off-chain `Deposit` EIP-712 (`depositIntoStrategyWithSignature`). `amount`
/// is in `token` units (user-legible); the minted shares are implied by the
/// strategy. `staker` is set for the signature variant; the submitter otherwise.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct DepositAction {
    /// Restaking venue.
    pub venue: RestakingVenue,
    /// Strategy contract receiving the deposit.
    #[tsify(type = "string")]
    pub strategy: Address,
    /// Underlying ERC-20 (LST) being deposited.
    pub token: TokenRef,
    /// Amount deposited, in `token` units.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Staker on whose behalf the deposit is made (signature variant); the
    /// submitter for the direct call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub staker: Option<Address>,
}
