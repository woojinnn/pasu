//! `Perp::DecreasePosition` lowering → `Perp::DecreasePositionContext`.

use serde_json::{Map, Value};

use policy_transition::action::perp::DecreasePerpAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_perp_venue, lower_size_spec};

/// Lower a `DecreasePerpAction` into the `Perp::DecreasePositionContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &DecreasePerpAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert(
        "positionId".into(),
        Value::String(action.position_id.clone()),
    );
    m.insert("size".into(), lower_size_spec(&action.size));
    m.insert(
        "slippageBp".into(),
        Value::from(i64::from(action.slippage_bp)),
    );
    // ClosePerpLiveInputs flattened.
    m.insert(
        "markPrice".into(),
        Value::String(li.mark_price.value.0.clone()),
    );
    m.insert(
        "unrealizedPnlNow".into(),
        Value::String(li.unrealized_pnl_now.value.to_string()),
    );
    m.insert(
        "fundingAccrued".into(),
        Value::String(li.funding_accrued.value.to_string()),
    );
    m.insert("feeBp".into(), Value::from(i64::from(li.fee_bp.value)));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"DecreasePosition""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::{Price, SignedI256};
    use policy_transition::action::perp::{
        ClosePerpLiveInputs, DecreasePerpAction, PerpAction, SizeSpec,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_size, sample_size_base, sample_size_quote,
        sample_venue,
    };

    /// Build a `DecreasePosition` body with the requested `size` spec.
    fn build(size: SizeSpec) -> ActionBody {
        let action = DecreasePerpAction {
            venue: sample_venue(),
            position_id: "pos-123".into(),
            size,
            slippage_bp: 50,
            live_inputs: ClosePerpLiveInputs {
                mark_price: live(Price::new("3050")),
                unrealized_pnl_now: live(SignedI256::try_from(125i64).unwrap()),
                funding_accrued: live(SignedI256::try_from(-7i64).unwrap()),
                fee_bp: live(5u32),
            },
        };
        ActionBody::Perp(PerpAction::DecreasePosition(action))
    }

    fn sample() -> (ActionBody, policy_transition::action::ActionMeta) {
        (build(sample_size()), onchain_meta())
    }

    #[test]
    fn decrease_position_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("decrease_position", &body, &meta);
    }

    /// `size = BaseAmount` (the `base_amount` arm of `lower_size_spec`).
    #[test]
    fn decrease_position_base_amount_size_conforms() {
        let body = build(sample_size_base());
        assert_conforms("decrease_position", &body, &onchain_meta());
    }

    /// `size = QuoteAmount` (the `quote_amount` arm of `lower_size_spec`).
    #[test]
    fn decrease_position_quote_amount_size_conforms() {
        let body = build(sample_size_quote());
        assert_conforms("decrease_position", &body, &onchain_meta());
    }
}
