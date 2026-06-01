//! `RemoveLiquidityAction` reducer — withdraw liquidity / burn LP or position.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::RemoveLiquidityAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for RemoveLiquidityAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        todo!()
    }
}
