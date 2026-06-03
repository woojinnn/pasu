//! Aave periphery adapters are high-risk bundled operations.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::PeripheryOperationAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for PeripheryOperationAction {
    /// Adapter calls can perform swaps, flash-loan callbacks, and lending state
    /// changes in one transaction. Preserve policy visibility without emitting
    /// an optimistic synthetic state transition.
    #[allow(clippy::unused_self)]
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
