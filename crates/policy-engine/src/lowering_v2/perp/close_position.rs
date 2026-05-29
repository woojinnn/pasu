//! `Perp::ClosePosition` lowering → `Perp::ClosePositionContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::ClosePerpAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_perp_venue, lower_size_spec};

/// Lower a `ClosePerpAction` into the `Perp::ClosePositionContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ClosePerpAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("positionId".into(), Value::String(action.position_id.clone()));
    // `size` is `None` for a full close — OMITTED when absent.
    if let Some(size) = &action.size {
        m.insert("size".into(), lower_size_spec(size));
    }
    m.insert("slippageBp".into(), Value::from(i64::from(action.slippage_bp)));
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

    Ok(ctx.lowered(r#"Perp::Action::"ClosePosition""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use simulation_reducer::action::perp::{ClosePerpAction, ClosePerpLiveInputs, PerpAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Price, SignedI256};

    use super::super::test_support::{assert_conforms, live, onchain_meta, sample_venue};

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = ClosePerpAction {
            venue: sample_venue(),
            position_id: "pos-123".into(),
            // Full close: `size` is None.
            size: None,
            slippage_bp: 50,
            live_inputs: ClosePerpLiveInputs {
                mark_price: live(Price::new("3050")),
                unrealized_pnl_now: live(SignedI256::try_from(125i64).unwrap()),
                funding_accrued: live(SignedI256::try_from(-7i64).unwrap()),
                fee_bp: live(5u32),
            },
        };
        (
            ActionBody::Perp(PerpAction::ClosePosition(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn close_position_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("close_position", &body, &meta);
    }
}
