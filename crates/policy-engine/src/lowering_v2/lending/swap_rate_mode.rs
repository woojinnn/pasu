//! `Lending::SwapRateMode` lowering → `Lending::SwapRateModeContext`.

use serde_json::{Map, Value};

use policy_transition::action::lending::SwapRateModeAction;

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
    use policy_state::primitives::{Decimal, U256};
    use policy_state::token::RateMode;
    use policy_transition::action::lending::{
        LendingAction, SwapRateModeAction, SwapRateModeLiveInputs,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{live, onchain_meta, usdc, venue};

    /// Build a `SwapRateMode` body targeting `new_mode`, holding the rest fixed.
    fn swap_body(new_mode: RateMode) -> ActionBody {
        ActionBody::Lending(LendingAction::SwapRateMode(SwapRateModeAction {
            venue: venue(),
            asset: usdc(),
            new_mode,
            live_inputs: SwapRateModeLiveInputs {
                current_debts: live((U256::from(0u64), U256::from(250_000_000u64))),
                rates: live((Decimal::new("0.0512"), Decimal::new("0.0689"))),
            },
        }))
    }

    /// A representative rate-mode swap to variable.
    #[test]
    fn swap_rate_mode_lowering_conforms_to_schema() {
        let body = swap_body(RateMode::Variable);
        super::super::test_support::assert_conforms("swap_rate_mode", &body, &onchain_meta());
    }

    /// `newMode == "stable"` — exercises the stable spelling of `rate_mode_str`.
    #[test]
    fn swap_rate_mode_to_stable_conforms() {
        let body = swap_body(RateMode::Stable);
        super::super::test_support::assert_conforms("swap_rate_mode", &body, &onchain_meta());
    }

    /// `newMode == "fixed"` — exercises the fixed spelling of `rate_mode_str`.
    #[test]
    fn swap_rate_mode_to_fixed_conforms() {
        let body = swap_body(RateMode::Fixed);
        super::super::test_support::assert_conforms("swap_rate_mode", &body, &onchain_meta());
    }
}
