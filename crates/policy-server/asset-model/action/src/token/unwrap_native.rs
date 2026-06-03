use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::U256;
use policy_state::token::TokenRef;

/// Native-currency unwrap (e.g. canonical WETH `withdraw(uint256)`) — burn the
/// 1:1 ERC20 wrapper back into the native gas currency.
///
/// `amount` is the `wad` argument (the wrapper amount unwrapped); the returned
/// native amount is 1:1. `token` identifies the wrapper contract being called
/// (`token.key.address` == the tx `to`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct UnwrapNativeAction {
    /// The wrapper token burned (e.g. WETH).
    pub token: TokenRef,
    /// Wrapper amount unwrapped back to native (the `wad` argument).
    #[tsify(type = "string")]
    pub amount: U256,
}
