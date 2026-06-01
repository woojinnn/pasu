//! `PerpAction` reducers.
//! One file per action; one file per venue's math.

mod adjust_margin;
mod cancel_order;
mod change_leverage;
mod change_margin_mode;
mod claim_funding;
mod close;
mod decrease;
mod increase;
mod open;
mod place_limit_order;
mod place_stop_order;

// Venue-specific math:
mod aevo;
mod drift;
mod dydx_v4;
mod gmx_v2;
mod hyperliquid;
mod jupiter_perps;
mod synthetix;
mod vertex;

// Cross-venue math primitives (PnL, funding, simple liq-price common form).
mod math;

// Shared per-action helpers (PendingTx id derivation, market venue tag).
mod common;

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::perp::PerpAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for PerpAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::OpenPosition(a) => a.apply(state, ctx),
            Self::ClosePosition(a) => a.apply(state, ctx),
            Self::IncreasePosition(a) => a.apply(state, ctx),
            Self::DecreasePosition(a) => a.apply(state, ctx),
            Self::AdjustMargin(a) => a.apply(state, ctx),
            Self::ChangeLeverage(a) => a.apply(state, ctx),
            Self::ChangeMarginMode(a) => a.apply(state, ctx),
            Self::PlaceLimitOrder(a) => a.apply(state, ctx),
            Self::PlaceStopOrder(a) => a.apply(state, ctx),
            Self::CancelOrder(a) => a.apply(state, ctx),
            Self::ClaimFunding(a) => a.apply(state, ctx),
        }
    }
}
