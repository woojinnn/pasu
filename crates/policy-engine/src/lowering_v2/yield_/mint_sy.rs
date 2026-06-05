//! `Yield::MintSy` lowering → `Yield::MintSyContext`.

use serde_json::{Map, Value};

use policy_transition::action::yield_::MintSyAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_yield_venue;

/// Lower a `Yield::MintSy` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &MintSyAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_yield_venue(&action.venue));
    m.insert("sy".into(), Value::String(addr(&action.sy)));
    m.insert(
        "externalToken".into(),
        lower_token_ref(&action.external_token),
    );
    m.insert(
        "netTokenIn".into(),
        Value::String(u256_hex(action.net_token_in)),
    );
    m.insert(
        "minSyOut".into(),
        Value::String(u256_hex(action.min_sy_out)),
    );
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));

    Ok(ctx.lowered(r#"Yield::Action::"MintSy""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::yield_::{MintSyAction, YieldAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, onchain_meta, pendle_market, pendle_venue, usdc, user,
    };

    #[test]
    fn mint_sy_conforms() {
        let body = ActionBody::Yield(YieldAction::MintSy(MintSyAction {
            venue: pendle_venue(),
            sy: pendle_market(),
            external_token: usdc(),
            net_token_in: U256::from(1_000_000_000u64),
            min_sy_out: U256::from(990_000_000u64),
            recipient: user(),
        }));
        assert_conforms("mint_sy", &body, &onchain_meta());
    }
}
