//! `TransferSharesAction` — transfer the staking token denominated in shares.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::LiveField;

use super::StakingVenue;

/// Transfer the rebasing staking token denominated in protocol shares.
///
/// Models stETH `transferShares(address _recipient, uint256 _sharesAmount)` and
/// `transferSharesFrom(address _sender, address _recipient, uint256 _sharesAmount)`.
/// `shares` is the protocol's internal share unit (NOT stETH balance units), so
/// this is distinct from a plain `erc20_transfer`. `from` is set for the
/// `…From` variant; the submitter is the source for the direct variant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TransferSharesAction {
    /// Liquid-staking venue.
    pub venue: StakingVenue,
    /// Recipient of the shares.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Amount in protocol shares (not stETH balance units).
    #[tsify(type = "string")]
    pub shares: U256,
    /// Source holder for the `…From` variant; the submitter for the direct variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub from: Option<Address>,
    /// Live inputs fetched at simulation time.
    pub live_inputs: TransferSharesLiveInputs,
}

/// Live-fetched inputs for a `TransferSharesAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TransferSharesLiveInputs {
    /// Pooled ETH (stETH balance) the transferred shares correspond to:
    /// stETH `getPooledEthByShares(shares)`. Turns the abstract `shares` unit
    /// into the stETH amount the recipient actually receives.
    pub pooled_eth: LiveField<U256>,
}
