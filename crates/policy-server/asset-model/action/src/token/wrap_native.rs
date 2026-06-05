use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::U256;
use policy_state::token::TokenRef;

/// Native-currency wrap (e.g. canonical WETH `deposit()`) — deposit native gas
/// currency into its 1:1 ERC20 wrapper.
///
/// `amount` is `msg.value` (the native amount wrapped); the minted wrapper
/// amount is 1:1 (unlike share-based liquid-staking wrappers, which live in the
/// `liquid_staking` domain). `token` identifies the wrapper contract being
/// called (`token.key.address` == the tx `to`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WrapNativeAction {
    /// The wrapper token minted (e.g. WETH).
    pub token: TokenRef,
    /// Native amount wrapped (`msg.value`).
    #[tsify(type = "string")]
    pub amount: U256,
}
