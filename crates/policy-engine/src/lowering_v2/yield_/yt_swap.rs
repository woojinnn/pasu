//! `Yield::YtSwap` lowering → `Yield::YtSwapContext`.

use serde_json::{Map, Value};

use policy_transition::action::yield_::YtSwapAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{enum_tag, lower_yield_venue};

/// Lower a `Yield::YtSwap` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &YtSwapAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_yield_venue(&action.venue));
    m.insert("market".into(), Value::String(addr(&action.market)));
    m.insert("direction".into(), enum_tag(&action.direction));
    if let Some(token) = &action.external_token {
        m.insert("externalToken".into(), lower_token_ref(token));
    }
    m.insert(
        "exactAmountIn".into(),
        Value::String(u256_hex(action.exact_amount_in)),
    );
    m.insert(
        "minAmountOut".into(),
        Value::String(u256_hex(action.min_amount_out)),
    );
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    // Market enrichment (P1c): SY/PT/YT from readTokens(), maturity from expiry().
    m.insert(
        "sy".into(),
        Value::String(addr(&action.live_inputs.sy.value)),
    );
    m.insert(
        "pt".into(),
        Value::String(addr(&action.live_inputs.pt.value)),
    );
    m.insert(
        "yt".into(),
        Value::String(addr(&action.live_inputs.yt.value)),
    );
    m.insert(
        "maturity".into(),
        Value::String(u256_hex(action.live_inputs.maturity.value)),
    );

    Ok(ctx.lowered(r#"Yield::Action::"YtSwap""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::yield_::{
        MarketTokensLiveInputs, YieldAction, YtSwapAction, YtSwapDirection,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, live_addr, live_u256, onchain_meta, pendle_market, pendle_venue, usdc,
        user,
    };

    fn market_tokens() -> MarketTokensLiveInputs {
        MarketTokensLiveInputs {
            sy: live_addr(),
            pt: live_addr(),
            yt: live_addr(),
            maturity: live_u256(),
        }
    }

    #[test]
    fn yt_swap_token_for_yt_conforms() {
        let body = ActionBody::Yield(YieldAction::YtSwap(YtSwapAction {
            venue: pendle_venue(),
            market: pendle_market(),
            direction: YtSwapDirection::TokenForYt,
            external_token: Some(usdc()),
            exact_amount_in: U256::from(1_000_000_000u64),
            min_amount_out: U256::from(1u64),
            recipient: user(),
            live_inputs: market_tokens(),
        }));
        assert_conforms("yt_swap", &body, &onchain_meta());
    }

    #[test]
    fn yt_swap_yt_for_sy_no_external_token_conforms() {
        let body = ActionBody::Yield(YieldAction::YtSwap(YtSwapAction {
            venue: pendle_venue(),
            market: pendle_market(),
            direction: YtSwapDirection::YtForSy,
            external_token: None,
            exact_amount_in: U256::from(100_000_000_000_000_000u64),
            min_amount_out: U256::from(1u64),
            recipient: user(),
            live_inputs: market_tokens(),
        }));
        assert_conforms("yt_swap", &body, &onchain_meta());
    }
}
