//! `Yield::ClaimYield` lowering → `Yield::ClaimYieldContext`.

use serde_json::{Map, Value};

use policy_state::primitives::Address;
use policy_transition::action::yield_::ClaimYieldAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_yield_venue;

/// Lower a `Vec<Address>` → a Cedar `Set<String>` (array of address strings).
fn lower_addr_set(addrs: &[Address]) -> Value {
    Value::Array(addrs.iter().map(|a| Value::String(addr(a))).collect())
}

/// Lower a `Yield::ClaimYield` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &ClaimYieldAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_yield_venue(&action.venue));
    m.insert("user".into(), Value::String(addr(&action.user)));
    m.insert("sys".into(), lower_addr_set(&action.sys));
    m.insert("yts".into(), lower_addr_set(&action.yts));
    m.insert("markets".into(), lower_addr_set(&action.markets));

    Ok(ctx.lowered(r#"Yield::Action::"ClaimYield""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::Address;
    use policy_transition::action::yield_::{ClaimYieldAction, YieldAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, onchain_meta, pendle_market, pendle_venue, user,
    };

    #[test]
    fn claim_yield_conforms() {
        let body = ActionBody::Yield(YieldAction::ClaimYield(ClaimYieldAction {
            venue: pendle_venue(),
            user: user(),
            sys: vec![Address::from_str("0xcbc72d92b2dc8187414f6734718563898740c0bc").unwrap()],
            yts: vec![Address::from_str("0x12cc7b6ee36b1a33ebc33dc41c39d383b3b33896").unwrap()],
            markets: vec![pendle_market()],
        }));
        assert_conforms("claim_yield", &body, &onchain_meta());
    }

    #[test]
    fn claim_yield_empty_sets_conforms() {
        let body = ActionBody::Yield(YieldAction::ClaimYield(ClaimYieldAction {
            venue: pendle_venue(),
            user: user(),
            sys: vec![],
            yts: vec![],
            markets: vec![pendle_market()],
        }));
        assert_conforms("claim_yield", &body, &onchain_meta());
    }
}
