//! `CollectFees` action — collect accrued, uncollected fees from a `Uniswap V3` / `V4` position NFT.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::LiveField;

use super::AmmVenue;

/// Collect accrued, uncollected fees from a `Uniswap V3` / `V4` position NFT.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CollectFeesAction {
    /// Pool venue holding the position.
    pub venue: AmmVenue,
    /// NFT position key.
    pub nft_key: TokenKey,
    /// Recipient of the collected fees.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Simulation-time fee accrual snapshot.
    pub live_inputs: CollectFeesLiveInputs,
}

/// Simulation-time inputs for a `CollectFees` action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CollectFeesLiveInputs {
    /// Fees owed to the position at simulation time.
    #[tsify(type = "LiveField<Array<[TokenRef, string]>>")]
    pub fees_owed: LiveField<Vec<(TokenRef, U256)>>,
}
