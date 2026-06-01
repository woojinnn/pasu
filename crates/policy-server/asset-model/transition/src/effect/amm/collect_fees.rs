//! `CollectFeesAction` reducer — `Uniswap V3`-style fee collection.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::CollectFeesAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for CollectFeesAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        todo!()
    }
}
