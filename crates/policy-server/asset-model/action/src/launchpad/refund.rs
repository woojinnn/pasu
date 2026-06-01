//! `Refund` action — refunds the committed payment token (oversubscription / failed sale).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ProtocolRef, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

/// Refunds the committed payment token (e.g. oversubscription or failed sale).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RefundAction {
    /// Launchpad platform (e.g. `CoinList`, `Buidlpad`, `Echo`, `Fjord`).
    pub platform: ProtocolRef,
    /// Identifier of the sale within the platform.
    pub sale_id: String,
    /// Address receiving the refunded tokens.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Live on-chain inputs read at execution time.
    pub live_inputs: RefundLiveInputs,
}

/// Live-read inputs required to execute a `RefundAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RefundLiveInputs {
    /// Amount eligible to be refunded.
    #[tsify(type = "LiveField<string>")]
    pub refund_amount: LiveField<U256>,
    /// `TokenRef` of the asset being refunded.
    pub refund_token: LiveField<TokenRef>,
}
