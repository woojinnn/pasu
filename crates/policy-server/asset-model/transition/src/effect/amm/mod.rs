//! `AmmAction` reducers.
//! One file per action; one file per venue's math.

mod add_liquidity;
mod aggregator;
mod balancer_v2;
mod balancer_v3;
mod collect_fees;
mod curve_v1;
mod curve_v2;
mod gsm_swap;
mod intent_order;
mod maverick_v2;
mod remove_liquidity;
mod sushi_v2;
mod swap;
mod trader_joe_lb;
mod uniswap_v2;
mod uniswap_v3;
mod uniswap_v4;

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::AmmAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for AmmAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Swap(a) => a.apply(state, ctx),
            Self::GsmSwap(a) => a.apply(state, ctx),
            Self::AddLiquidity(a) => a.apply(state, ctx),
            Self::RemoveLiquidity(a) => a.apply(state, ctx),
            Self::CollectFees(a) => a.apply(state, ctx),
            Self::SignIntentOrder(a) => a.apply(state, ctx),
            Self::SettleIntentOrder(a) => a.apply(state, ctx),
            Self::CancelIntentOrder(a) => a.apply(state, ctx),
        }
    }
}
