//! `Yield::SignLimitOrder` lowering → `Yield::SignLimitOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::yield_::SignLimitOrderAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{enum_tag, lower_yield_venue};

/// Lower a `Yield::SignLimitOrder` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &SignLimitOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_yield_venue(&action.venue));
    m.insert("orderType".into(), enum_tag(&action.order_type));
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("yt".into(), Value::String(addr(&action.yt)));
    m.insert("maker".into(), Value::String(addr(&action.maker)));
    m.insert("receiver".into(), Value::String(addr(&action.receiver)));
    m.insert(
        "makingAmount".into(),
        Value::String(u256_hex(action.making_amount)),
    );
    m.insert("expiry".into(), Value::String(u256_hex(action.expiry)));

    Ok(ctx.lowered(r#"Yield::Action::"SignLimitOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::yield_::{LimitOrderType, SignLimitOrderAction, YieldAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, onchain_meta, pendle_market, pendle_venue, usdc, user,
    };

    #[test]
    fn sign_limit_order_sy_for_pt_conforms() {
        let body = ActionBody::Yield(YieldAction::SignLimitOrder(SignLimitOrderAction {
            venue: pendle_venue(),
            order_type: LimitOrderType::SyForPt,
            token: usdc(),
            yt: pendle_market(),
            maker: user(),
            receiver: user(),
            making_amount: U256::from(1_000_000_000u64),
            expiry: U256::from(1_800_000_000u64),
        }));
        // Conformance validates context SHAPE; ActionMeta type is nature-agnostic,
        // so onchain_meta() suffices here (every domain's gate uses it).
        assert_conforms("sign_limit_order", &body, &onchain_meta());
    }
}
