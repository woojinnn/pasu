//! `Yield::PtSwap` lowering → `Yield::PtSwapContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::yield_::PtSwapAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{enum_tag, lower_yield_venue};

/// Lower a `Yield::PtSwap` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &PtSwapAction,
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

    Ok(ctx.lowered(r#"Yield::Action::"PtSwap""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::yield_::{
        MarketTokensLiveInputs, PtSwapAction, PtSwapDirection, YieldAction,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;

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
    fn pt_swap_token_for_pt_conforms() {
        let body = ActionBody::Yield(YieldAction::PtSwap(PtSwapAction {
            venue: pendle_venue(),
            market: pendle_market(),
            direction: PtSwapDirection::TokenForPt,
            external_token: Some(usdc()),
            exact_amount_in: U256::from(1_000_000_000u64),
            min_amount_out: U256::from(990_000_000u64),
            recipient: user(),
            live_inputs: market_tokens(),
        }));
        assert_conforms("pt_swap", &body, &onchain_meta());
    }

    #[test]
    fn pt_swap_pt_for_sy_no_external_token_conforms() {
        // SY-side direction: external_token omitted.
        let body = ActionBody::Yield(YieldAction::PtSwap(PtSwapAction {
            venue: pendle_venue(),
            market: pendle_market(),
            direction: PtSwapDirection::PtForSy,
            external_token: None,
            exact_amount_in: U256::from(500_000_000_000_000_000u64),
            min_amount_out: U256::from(490_000_000_000_000_000u64),
            recipient: user(),
            live_inputs: market_tokens(),
        }));
        assert_conforms("pt_swap", &body, &onchain_meta());
    }
}
