use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

/// `ERC20` `transfer(recipient, amount)` — direct token transfer from the actor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Erc20TransferAction {
    /// Token being transferred.
    pub token: TokenRef,
    /// Address receiving the tokens.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Amount to transfer.
    #[tsify(type = "string")]
    pub amount: U256,
    /// `true` when this transfer is a protocol/router EGRESS of its OWN held
    /// balance to `recipient` (e.g. Uniswap `sweepToken` / `unwrapWETH9`), NOT a
    /// direct user send. Defaults to `false` (a normal user transfer) and is
    /// omitted from the wire when false, so existing decodes are byte-identical.
    /// A redirected egress (`recipient != signer`) hidden inside a swap multicall
    /// can siphon the swap output; a policy gates THAT case via this flag without
    /// warning on ordinary user sends.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_router_egress: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde `skip_serializing_if` needs `&bool`.
fn is_false(value: &bool) -> bool {
    !*value
}
