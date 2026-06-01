//! `Commit` action — subscribes funds to a launchpad sale.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, Price, ProtocolRef, U256};
use simulation_state::token::TokenRef;
use simulation_state::LiveField;

use super::SaleState;

/// Commits funds to a launchpad sale (subscription).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CommitAction {
    /// Launchpad platform (e.g. `CoinList`, `Buidlpad`, `Echo`, `Fjord`).
    pub platform: ProtocolRef,
    /// Identifier of the sale within the platform.
    pub sale_id: String,
    /// Token used to pay into the sale (e.g. stablecoin or native asset).
    pub pay_token: TokenRef,
    /// Amount of `pay_token` to commit.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Address receiving the resulting allocation/claim rights.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Live on-chain inputs read at execution time.
    pub live_inputs: CommitLiveInputs,
}

/// Live-read inputs required to validate and execute a `CommitAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct CommitLiveInputs {
    /// Current sale state (cap, window, vest schedule, totals).
    pub sale_state: LiveField<SaleState>,
    /// Per-user commit cap enforced by the platform.
    #[tsify(type = "LiveField<string>")]
    pub user_cap: LiveField<U256>,
    /// Amount already committed by the user.
    #[tsify(type = "LiveField<string>")]
    pub user_committed: LiveField<U256>,
    /// Expected sale price (if the platform exposes one) used for slippage/UI checks.
    pub expected_token_price: LiveField<Option<Price>>,
}
