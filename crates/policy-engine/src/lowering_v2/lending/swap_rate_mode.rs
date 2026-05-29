//! `Lending::SwapRateMode` lowering → `Lending::SwapRateModeContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::SwapRateModeAction;

use super::super::common::cedar::u256_hex;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, rate_mode_str};

/// Lower a `Lending::SwapRateMode` action into the `Lending::SwapRateModeContext`
/// shape. The Rust `(variable, stable)` tuples are flattened to separately-named
/// string fields (Cedar has no tuple type).
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &SwapRateModeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let current_debts = &action.live_inputs.current_debts.value;
    let rates = &action.live_inputs.rates.value;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    m.insert(
        "newMode".into(),
        Value::String(rate_mode_str(&action.new_mode).into()),
    );
    // `(variable, stable)` tuples → named fields, index-aligned.
    m.insert(
        "currentDebtVariable".into(),
        Value::String(u256_hex(current_debts.0)),
    );
    m.insert(
        "currentDebtStable".into(),
        Value::String(u256_hex(current_debts.1)),
    );
    m.insert("rateVariable".into(), Value::String(rates.0.to_string()));
    m.insert("rateStable".into(), Value::String(rates.1.to_string()));
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"SwapRateMode""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::lending::{
        LendingAction, SwapRateModeAction, SwapRateModeLiveInputs,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Decimal, U256};
    use simulation_state::token::RateMode;

    use super::super::test_support::{live, onchain_meta, usdc, venue};

    /// A representative rate-mode swap to variable.
    #[test]
    fn swap_rate_mode_lowering_conforms_to_schema() {
        let action = LendingAction::SwapRateMode(SwapRateModeAction {
            venue: venue(),
            asset: usdc(),
            new_mode: RateMode::Variable,
            live_inputs: SwapRateModeLiveInputs {
                current_debts: live((U256::from(0u64), U256::from(250_000_000u64))),
                rates: live((Decimal::new("0.0512"), Decimal::new("0.0689"))),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("swap_rate_mode", &body, &meta);
    }
}
