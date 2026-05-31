//! `UnlockAction` — withdraw an expired vote-escrow lock (Curve veCRV `withdraw`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::token::TokenRef;

use super::StakeVenue;

/// Withdraw a fully-expired vote-escrow lock, releasing the locked token.
///
/// Models Curve `VotingEscrow.withdraw()` (no args). The whole locked balance
/// is released — the amount is not in calldata, so it is not represented here.
/// `token` is the released token (e.g. CRV).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct UnlockAction {
    /// Staking / vote-escrow venue (e.g. Curve `VotingEscrow`).
    pub venue: StakeVenue,
    /// Token released by the withdrawal (e.g. CRV).
    pub token: TokenRef,
}
