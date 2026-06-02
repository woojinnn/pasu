//! `SetAuthorizationAction` reducer.
//!
//! A permission grant changes no token balances and opens/closes no position —
//! it authorizes `authorized` to act on the submitter's behalf across the
//! protocol. The simulation state models balances/positions, not the protocol's
//! operator-authorization map, so this reducer is a deterministic no-op
//! (`StateDelta::new()`). The security-relevant evaluation happens in the Cedar
//! policy layer (a policy can deny granting control to an untrusted operator).

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::SetAuthorizationAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for SetAuthorizationAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        // Operator-authorization grant — no balance/position delta in the model.
        Ok(StateDelta::new())
    }
}
