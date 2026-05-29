//! `Lending::DelegateBorrow` lowering → `Lending::DelegateBorrowContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::DelegateBorrowAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, rate_mode_str};

/// Lower a `Lending::DelegateBorrow` action into the
/// `Lending::DelegateBorrowContext` shape. This action carries no live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &DelegateBorrowAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    m.insert("delegatee".into(), Value::String(addr(&action.delegatee)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` are host-populated → omitted.
    m.insert(
        "rateMode".into(),
        Value::String(rate_mode_str(&action.rate_mode).into()),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"DelegateBorrow""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::lending::{DelegateBorrowAction, LendingAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;
    use simulation_state::token::RateMode;

    use super::super::test_support::{offchain_meta, other, usdc, venue};

    /// Build a `DelegateBorrow` body targeting `rate_mode`, holding the rest
    /// fixed. This action carries no live inputs.
    fn delegate_body(rate_mode: RateMode) -> ActionBody {
        ActionBody::Lending(LendingAction::DelegateBorrow(DelegateBorrowAction {
            venue: venue(),
            asset: usdc(),
            delegatee: other(),
            amount: U256::from(1_000_000_000u64),
            rate_mode,
        }))
    }

    /// A representative credit delegation of USDC to another address. Credit
    /// delegation is a signature action, so this uses the OFF-CHAIN meta — which
    /// also widens the lending gate to prove a lending context conforms with
    /// `nature = offchain_sig` (exercising `lower_eip712` + `nonceKey`).
    #[test]
    fn delegate_borrow_lowering_conforms_to_schema() {
        let body = delegate_body(RateMode::Variable);
        super::super::test_support::assert_conforms("delegate_borrow", &body, &offchain_meta());
    }

    /// `rateMode == "stable"` — exercises the stable spelling.
    #[test]
    fn delegate_borrow_stable_rate_conforms() {
        let body = delegate_body(RateMode::Stable);
        super::super::test_support::assert_conforms("delegate_borrow", &body, &offchain_meta());
    }

    /// `rateMode == "fixed"` — exercises the fixed spelling.
    #[test]
    fn delegate_borrow_fixed_rate_conforms() {
        let body = delegate_body(RateMode::Fixed);
        super::super::test_support::assert_conforms("delegate_borrow", &body, &offchain_meta());
    }
}
