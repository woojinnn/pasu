//! `BuyCollateralAction` reducer.
//!
//! The action is security-relevant as a signed intent, but this reducer does not
//! yet model Comet reserve accounting or token-balance effects. Policy lowering
//! still exposes the deterministic trade parameters for allow/deny decisions.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::BuyCollateralAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for BuyCollateralAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
