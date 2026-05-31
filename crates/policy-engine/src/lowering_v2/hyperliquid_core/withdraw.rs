//! `HyperliquidCore::HlWithdraw` lowering → `HyperliquidCore::HlWithdrawContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::hyperliquid_core::HlWithdrawAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlWithdrawAction` into the `HyperliquidCore::HlWithdrawContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlWithdrawAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "destination".into(),
        Value::String(addr(&action.destination)),
    );
    m.insert("amount".into(), Value::String(action.amount.0.clone()));

    Ok(ctx.lowered(r#"HyperliquidCore::Action::"HlWithdraw""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::hyperliquid_core::{HlWithdrawAction, HyperliquidCoreAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Address, Decimal};

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn withdraw_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::Withdraw(HlWithdrawAction {
            destination: Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap(),
            amount: Decimal::new("1000.50"),
        }));
        assert_conforms("hl_withdraw", &body, &offchain_meta());
    }
}
