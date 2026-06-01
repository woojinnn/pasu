//! `WithdrawCommit` action — cancels a prior commitment on platforms that allow pre-sale withdrawal.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{ProtocolRef, U256};
use simulation_state::LiveField;

use super::SaleState;

/// Cancels a prior commitment on platforms that allow pre-sale withdrawal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WithdrawCommitAction {
    /// Launchpad platform (e.g. `CoinList`, `Buidlpad`, `Echo`, `Fjord`).
    pub platform: ProtocolRef,
    /// Identifier of the sale within the platform.
    pub sale_id: String,
    /// Amount to withdraw; `None` withdraws the full committed balance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub amount: Option<U256>,
    /// Live on-chain inputs read at execution time.
    pub live_inputs: WithdrawCommitLiveInputs,
}

/// Live-read inputs required to execute a `WithdrawCommitAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WithdrawCommitLiveInputs {
    /// Amount currently available to withdraw.
    #[tsify(type = "LiveField<string>")]
    pub withdrawable: LiveField<U256>,
    /// Current sale state used to verify withdrawal is still permitted.
    pub sale_state: LiveField<SaleState>,
}
