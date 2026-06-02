//! `ClaimAllocation` action — claims allocated sale tokens after the sale ends.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ProtocolRef, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

/// Claims the allocated sale tokens after the sale ends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimAllocationAction {
    /// Launchpad platform (e.g. `CoinList`, `Buidlpad`, `Echo`, `Fjord`).
    pub platform: ProtocolRef,
    /// Identifier of the sale within the platform.
    pub sale_id: String,
    /// Address receiving the claimed allocation.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Live on-chain inputs read at execution time.
    pub live_inputs: ClaimAllocationLiveInputs,
}

/// Live-read inputs required to execute a `ClaimAllocationAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimAllocationLiveInputs {
    /// Allocated `(TokenRef, amount)` granted to the user.
    #[tsify(type = "LiveField<[TokenRef, string]>")]
    pub allocated: LiveField<(TokenRef, U256)>,
    /// Refund owed due to oversubscription.
    #[tsify(type = "LiveField<string>")]
    pub refund_due: LiveField<U256>,
    /// Whether the allocation is currently claimable.
    pub is_claimable: LiveField<bool>,
}
