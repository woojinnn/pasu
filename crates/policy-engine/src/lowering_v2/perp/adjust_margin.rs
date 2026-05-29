//! `Perp::AdjustMargin` lowering → `Perp::AdjustMarginContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::{AdjustMarginAction, PerpPositionLive};

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_perp_venue;

/// Lower an `AdjustMarginAction` into the `Perp::AdjustMarginContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &AdjustMarginAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("positionId".into(), Value::String(action.position_id.clone()));
    // `delta` is a signed SignedI256: positive = deposit, negative = withdraw.
    m.insert("delta".into(), Value::String(action.delta.to_string()));
    // AdjustMarginLiveInputs flattened.
    m.insert(
        "positionState".into(),
        lower_perp_position_live(&li.position_state.value),
    );
    m.insert(
        "freeMarginAfter".into(),
        Value::String(u256_hex(li.free_margin_after.value)),
    );
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"AdjustMargin""#, Value::Object(m)))
}

/// Lower a [`PerpPositionLive`] → `{ sizeBase, notionalUsd, entryPrice,
/// markPrice, liqPrice?, unrealizedPnl }` (`Perp::PerpPositionLive`). Used only
/// by `AdjustMargin`, so it lives in this leaf. `liqPrice` is omitted when
/// absent; `unrealizedPnl` is a `SignedI256` rendered as a signed string.
fn lower_perp_position_live(pos: &PerpPositionLive) -> Value {
    let mut m = Map::new();
    m.insert("sizeBase".into(), Value::String(u256_hex(pos.size_base)));
    m.insert(
        "notionalUsd".into(),
        Value::String(u256_hex(pos.notional_usd)),
    );
    m.insert("entryPrice".into(), Value::String(pos.entry_price.0.clone()));
    m.insert("markPrice".into(), Value::String(pos.mark_price.0.clone()));
    if let Some(liq_price) = &pos.liq_price {
        m.insert("liqPrice".into(), Value::String(liq_price.0.clone()));
    }
    m.insert(
        "unrealizedPnl".into(),
        Value::String(pos.unrealized_pnl.to_string()),
    );
    Value::Object(m)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use simulation_reducer::action::perp::{AdjustMarginAction, AdjustMarginLiveInputs, PerpAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{SignedI256, U256};

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_position_live, sample_venue,
    };

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = AdjustMarginAction {
            venue: sample_venue(),
            position_id: "pos-123".into(),
            // Withdraw 100 (negative delta) to exercise the signed path.
            delta: SignedI256::try_from(-100i64).unwrap(),
            live_inputs: AdjustMarginLiveInputs {
                position_state: live(sample_position_live()),
                free_margin_after: live(U256::from(7_900_000_000u64)),
            },
        };
        (
            ActionBody::Perp(PerpAction::AdjustMargin(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn adjust_margin_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("adjust_margin", &body, &meta);
    }
}
