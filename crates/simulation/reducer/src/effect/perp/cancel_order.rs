//! `CancelOrderAction` reducer — cancel a previously placed limit or stop order.
//!
//! ## Effect
//!
//! Emits a `PendingChange::Remove { id, reason: Cancelled }`. The `id`
//! references the order's venue-assigned identifier (`self.order_id`),
//! which downstream `apply_delta` matches against the `PendingTx.id`
//! stored at submission time.
//!
//! ## Pending-id matching
//!
//! For venue-managed orders (`Hyperliquid` / `Aevo`) `self.order_id` is the
//! venue's own id (e.g. Hyperliquid `oid: u64` as string). For
//! reducer-synthesized orders (where we built the id via
//! `common::pending_id_for_limit_order`) the caller passes the synthesized
//! string verbatim. The reducer does not validate the id exists in
//! `state.pending` — that lives in the policy layer, which has the full
//! pending table and can warn on a no-op cancel. Reducer-side we just
//! record the Remove intent.

use simulation_state::delta::PendingRemoveReason;
use simulation_state::{EvalContext, PendingChange, StateDelta, WalletState};

use crate::action::perp::CancelOrderAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for CancelOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        delta.pending_changes.push(PendingChange::Remove {
            id: self.order_id.clone(),
            reason: PendingRemoveReason::Cancelled,
        });
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::primitives::{Address, ChainId, Time};
    use simulation_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::PerpVenue;

    fn ctx() -> EvalContext {
        use simulation_state::eval_context::RequestKind;
        EvalContext::new(
            ChainId::ethereum_mainnet(),
            Time::from_unix(1_738_000_000),
            RequestKind::Transaction,
        )
    }

    fn state() -> WalletState {
        let addr = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
        WalletState::new(WalletId::new(addr, [ChainId::ethereum_mainnet()]))
    }

    /// Cancel emits a single `PendingChange::Remove` with Cancelled reason.
    #[test]
    fn cancel_emits_remove_with_cancelled_reason() {
        let action = CancelOrderAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            order_id: "limit:hyperliquid:ETH-PERP:long:2950".into(),
        };
        let delta = action.apply(&state(), &ctx()).unwrap();
        assert_eq!(delta.pending_changes.len(), 1);
        match &delta.pending_changes[0] {
            PendingChange::Remove { id, reason } => {
                assert_eq!(id, "limit:hyperliquid:ETH-PERP:long:2950");
                assert!(matches!(reason, PendingRemoveReason::Cancelled));
            }
            _ => panic!("expected Remove"),
        }
    }

    /// Cancel does not touch wallet balances or positions.
    #[test]
    fn cancel_emits_no_other_changes() {
        let action = CancelOrderAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            order_id: "any-id".into(),
        };
        let delta = action.apply(&state(), &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
        assert!(delta.position_changes.is_empty());
    }
}
