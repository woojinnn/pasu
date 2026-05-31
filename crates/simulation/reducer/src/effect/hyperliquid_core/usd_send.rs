//! `hl_usd_send` reducer — a USDC transfer to another account. Mechanically an
//! outflow from the perp account, so it reuses the withdraw outflow body.

use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::hyperliquid_core::HlUsdSendAction;
use crate::error::ReducerResult;

use super::withdraw;

pub(super) fn apply(
    action: &HlUsdSendAction,
    state: &WalletState,
    ctx: &EvalContext,
) -> ReducerResult<StateDelta> {
    withdraw::apply_outflow(&action.amount, state, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    use simulation_state::eval_context::RequestKind;
    use simulation_state::position::PositionKind;
    use simulation_state::primitives::{Address, ChainId, Decimal, Time};
    use simulation_state::wallet::{WalletId, WalletState};
    use simulation_state::PositionChange;

    fn ctx() -> EvalContext {
        EvalContext::new(
            ChainId::ethereum_mainnet(),
            Time::from_unix(1),
            RequestKind::Signature,
        )
    }
    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from([0u8; 20]),
            std::iter::empty::<ChainId>(),
        ))
    }

    #[test]
    fn usd_send_on_empty_base_records_outflow_intent() {
        let action = HlUsdSendAction {
            destination: Address::from([0xaa; 20]),
            amount: Decimal::new("50"),
        };
        let delta = apply(&action, &empty_state(), &ctx()).unwrap();
        match &delta.position_changes[0] {
            PositionChange::Open { position } => match &position.kind {
                PositionKind::HyperliquidAccount(a) => {
                    assert_eq!(a.pending_outflow, Decimal::new("50"));
                }
                o => panic!("{o:?}"),
            },
            o => panic!("{o:?}"),
        }
    }
}
