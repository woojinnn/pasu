//! `PermissionAction` reducers.
//!
//! Permission grants change no token balances or modeled positions directly.
//! They are still policy-critical, so the action is represented for Cedar
//! evaluation while the reducer returns an empty delta.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::permission::{PermissionAction, ProtocolAuthorizationAction};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for PermissionAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::ProtocolAuthorization(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for ProtocolAuthorizationAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
