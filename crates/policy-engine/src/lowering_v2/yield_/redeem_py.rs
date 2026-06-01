//! `Yield::RedeemPy` lowering → `Yield::RedeemPyContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::yield_::RedeemPyAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_yield_venue;

/// Lower a `Yield::RedeemPy` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &RedeemPyAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_yield_venue(&action.venue));
    m.insert("yt".into(), Value::String(addr(&action.yt)));
    if let Some(token) = &action.external_token {
        m.insert("externalToken".into(), lower_token_ref(token));
    }
    m.insert("netPyIn".into(), Value::String(u256_hex(action.net_py_in)));
    m.insert(
        "minOutput".into(),
        Value::String(u256_hex(action.min_output)),
    );
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));

    Ok(ctx.lowered(r#"Yield::Action::"RedeemPy""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::yield_::{RedeemPyAction, YieldAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;

    use super::super::test_support::{
        assert_conforms, onchain_meta, pendle_market, pendle_venue, usdc, user,
    };

    #[test]
    fn redeem_py_to_token_conforms() {
        let body = ActionBody::Yield(YieldAction::RedeemPy(RedeemPyAction {
            venue: pendle_venue(),
            yt: pendle_market(),
            external_token: Some(usdc()),
            net_py_in: U256::from(1_000_000_000u64),
            min_output: U256::from(990_000_000u64),
            recipient: user(),
        }));
        assert_conforms("redeem_py", &body, &onchain_meta());
    }

    #[test]
    fn redeem_py_to_sy_no_external_token_conforms() {
        let body = ActionBody::Yield(YieldAction::RedeemPy(RedeemPyAction {
            venue: pendle_venue(),
            yt: pendle_market(),
            external_token: None,
            net_py_in: U256::from(1_000_000_000_000_000_000u64),
            min_output: U256::from(990_000_000_000_000_000u64),
            recipient: user(),
        }));
        assert_conforms("redeem_py", &body, &onchain_meta());
    }
}
