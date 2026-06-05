//! `Launchpad::Refund` lowering → `Launchpad::RefundContext`.

use serde_json::{Map, Value};

use policy_transition::action::launchpad::RefundAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_protocol_ref;

/// Lower a `Launchpad::Refund` action into the `Launchpad::RefundContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &RefundAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("platform".into(), lower_protocol_ref(&action.platform));
    m.insert("saleId".into(), Value::String(action.sale_id.clone()));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert(
        "refundAmount".into(),
        Value::String(u256_hex(action.live_inputs.refund_amount.value)),
    );
    // `refundAmountNano` / `refundAmountUsd` are host-populated — OMITTED.
    m.insert(
        "refundToken".into(),
        lower_token_ref(&action.live_inputs.refund_token.value),
    );
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Launchpad::Action::"Refund""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_state::primitives::U256;
    use policy_state::LiveField;
    use policy_transition::action::launchpad::{LaunchpadAction, RefundAction, RefundLiveInputs};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{now, platform, src, usdc, user};

    /// A representative on-chain `Refund`: a refund amount and the refunded
    /// token (the ERC20 pay token).
    fn sample() -> (ActionBody, policy_transition::action::ActionMeta) {
        let action = RefundAction {
            platform: platform(),
            sale_id: "sale-42".into(),
            recipient: user(),
            live_inputs: RefundLiveInputs {
                refund_amount: LiveField::new(U256::from(50_000_000u64), src(), now()),
                refund_token: LiveField::new(usdc(), src(), now()),
            },
        };
        (
            ActionBody::Launchpad(LaunchpadAction::Refund(action)),
            super::super::test_support::onchain_meta(),
        )
    }

    #[test]
    fn refund_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        super::super::test_support::assert_conforms("refund", &body, &meta);
    }
}
