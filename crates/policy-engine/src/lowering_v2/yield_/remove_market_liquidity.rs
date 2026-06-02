//! `Yield::RemoveMarketLiquidity` lowering → `Yield::RemoveMarketLiquidityContext`.

use serde_json::{Map, Value};

use policy_transition::action::yield_::RemoveMarketLiquidityAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{enum_tag, lower_yield_venue};

/// Lower a `Yield::RemoveMarketLiquidity` action.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &RemoveMarketLiquidityAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_yield_venue(&action.venue));
    m.insert("market".into(), Value::String(addr(&action.market)));
    m.insert("kind".into(), enum_tag(&action.kind));
    if let Some(token) = &action.external_token {
        m.insert("externalToken".into(), lower_token_ref(token));
    }
    m.insert("netLpIn".into(), Value::String(u256_hex(action.net_lp_in)));
    m.insert(
        "minTokenOut".into(),
        Value::String(u256_hex(action.min_token_out)),
    );
    m.insert(
        "minPtOut".into(),
        Value::String(u256_hex(action.min_pt_out)),
    );
    m.insert(
        "minSyOut".into(),
        Value::String(u256_hex(action.min_sy_out)),
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

    Ok(ctx.lowered(
        r#"Yield::Action::"RemoveMarketLiquidity""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::yield_::{
        MarketTokensLiveInputs, RemoveLiquidityKind, RemoveMarketLiquidityAction, YieldAction,
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
    fn remove_single_token_conforms() {
        let body = ActionBody::Yield(YieldAction::RemoveMarketLiquidity(
            RemoveMarketLiquidityAction {
                venue: pendle_venue(),
                market: pendle_market(),
                kind: RemoveLiquidityKind::SingleToken,
                external_token: Some(usdc()),
                net_lp_in: U256::from(1_000_000_000_000_000_000u64),
                min_token_out: U256::from(900_000_000u64),
                min_pt_out: U256::ZERO,
                min_sy_out: U256::ZERO,
                recipient: user(),
                live_inputs: market_tokens(),
            },
        ));
        assert_conforms("remove_market_liquidity", &body, &onchain_meta());
    }

    #[test]
    fn remove_dual_sy_and_pt_no_external_token_conforms() {
        let body = ActionBody::Yield(YieldAction::RemoveMarketLiquidity(
            RemoveMarketLiquidityAction {
                venue: pendle_venue(),
                market: pendle_market(),
                kind: RemoveLiquidityKind::DualSyAndPt,
                external_token: None,
                net_lp_in: U256::from(1_000_000_000_000_000_000u64),
                min_token_out: U256::ZERO,
                min_pt_out: U256::from(1u64),
                min_sy_out: U256::from(1u64),
                recipient: user(),
                live_inputs: market_tokens(),
            },
        ));
        assert_conforms("remove_market_liquidity", &body, &onchain_meta());
    }
}
