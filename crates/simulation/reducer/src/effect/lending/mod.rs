//! `LendingAction` reducers.
//!
//! One file per action; one file per venue's math.

mod borrow;
mod delegate_borrow;
mod liquidate;
mod repay;
mod set_collateral;
mod set_emode;
mod supply;
mod swap_rate_mode;
mod withdraw;

// Venue-specific math:
mod aave_v2;
mod aave_v3;
mod compound_v2;
mod compound_v3;
mod fluid;
mod morpho_blue;
mod morpho_optimizer;
mod shared;
mod spark;

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::LendingAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for LendingAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Supply(a) => a.apply(state, ctx),
            Self::Withdraw(a) => a.apply(state, ctx),
            Self::Borrow(a) => a.apply(state, ctx),
            Self::Repay(a) => a.apply(state, ctx),
            Self::SwapRateMode(a) => a.apply(state, ctx),
            Self::SetEMode(a) => a.apply(state, ctx),
            Self::EnableCollateral(a) => set_collateral::apply(a, state, ctx, true),
            Self::DisableCollateral(a) => set_collateral::apply(a, state, ctx, false),
            Self::DelegateBorrow(a) => a.apply(state, ctx),
            Self::Liquidate(a) => a.apply(state, ctx),
        }
    }
}
