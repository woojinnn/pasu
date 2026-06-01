//! `AddLiquidityAction` reducer — deposit liquidity into a pool.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::AddLiquidityAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for AddLiquidityAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        todo!()
    }
}
