//! `GsmSwap` reducer.
//!
//! A GSM swap moves GHO ↔ the GSM asset at a fee/price-strategy-determined
//! rate. Like the other onboarding-scope reducers (`liquid_staking`,
//! `staking`), precise balance accounting is deferred — the decode-path
//! `ActionBody` and the Cedar policy layer are the scope; the reducer is a
//! deterministic no-op.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::GsmSwapAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for GsmSwapAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
