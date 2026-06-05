//! `SetAuthorizationAction` — grant or revoke an operator's full control over
//! the submitter's positions in a lending protocol.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ChainId};

/// Grant or revoke a manager/operator's authorization over the submitter's
/// positions in a lending protocol.
///
/// Unlike the other lending actions this is **account-wide, not market-scoped**
/// — it authorizes `authorized` to act on the submitter's behalf across the
/// whole protocol, so it carries a bespoke `{ chain, protocol }` locator
/// instead of a market-bearing [`super::LendingVenue`]. Models Morpho Blue's
/// `setAuthorization(address,bool)` (on-chain) and the `Authorization` EIP-712
/// consumed by `setAuthorizationWithSig` (off-chain). A pure permission grant —
/// no live inputs (cf. `DelegateBorrowAction`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SetAuthorizationAction {
    /// Chain hosting the protocol.
    pub chain: ChainId,
    /// Protocol contract whose positions `authorized` gains control over
    /// (e.g. the Morpho Blue singleton).
    #[tsify(type = "string")]
    pub protocol: Address,
    /// Account granting/revoking permission, when it is explicit in calldata or
    /// EIP-712. Direct calls usually omit this because the submitter is the
    /// authorizer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub authorizer: Option<Address>,
    /// Address being granted / revoked full control of the submitter's positions.
    #[tsify(type = "string")]
    pub authorized: Address,
    /// `true` = grant authorization, `false` = revoke.
    pub is_authorized: bool,
}
