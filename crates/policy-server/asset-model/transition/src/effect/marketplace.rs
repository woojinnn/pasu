//! `MarketplaceAction` reducers.
//!
//! Marketplace orders (Seaport) are settled by counterparties: signing or
//! cancelling an order does not change the wallet's own balance state, and a
//! taker's fulfill is a barter the balance-level simulator does not model.
//! Reducers are deterministic no-ops — the structured `ActionBody` is what the
//! policy engine inspects.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::marketplace::{
    CancelOrderAction, FulfillOrderAction, MarketplaceAction, SignOrderAction,
};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for MarketplaceAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::SignOrder(a) => a.apply(state, ctx),
            Self::FulfillOrder(a) => a.apply(state, ctx),
            Self::CancelOrder(a) => a.apply(state, ctx),
        }
    }
}

impl Reducer for SignOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for FulfillOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}

impl Reducer for CancelOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        Ok(StateDelta::new())
    }
}
