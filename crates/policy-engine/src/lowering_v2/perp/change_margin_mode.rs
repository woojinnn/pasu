//! `Perp::ChangeMarginMode` lowering → `Perp::ChangeMarginModeContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::ChangeMarginModeAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_market_ref, lower_perp_venue, margin_mode};

/// Lower a `ChangeMarginModeAction` into the `Perp::ChangeMarginModeContext`
/// shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ChangeMarginModeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("market".into(), lower_market_ref(&action.market));
    m.insert(
        "newMode".into(),
        Value::String(margin_mode(&action.new_mode).into()),
    );
    // ChangeMarginModeLiveInputs flattened.
    // `affectedPositions` (Vec<PositionId>) → Set<String>.
    let affected: Vec<Value> = li
        .affected_positions
        .value
        .iter()
        .map(|id| Value::String(id.clone()))
        .collect();
    m.insert("affectedPositions".into(), Value::Array(affected));
    // `marginReallocation` (Vec<(PositionId, U256)>) →
    // Set<{ positionId, amount }>.
    let realloc: Vec<Value> = li
        .margin_reallocation
        .value
        .iter()
        .map(|(id, amount)| {
            let mut e = Map::new();
            e.insert("positionId".into(), Value::String(id.clone()));
            e.insert("amount".into(), Value::String(u256_hex(*amount)));
            Value::Object(e)
        })
        .collect();
    m.insert("marginReallocation".into(), Value::Array(realloc));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"ChangeMarginMode""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use simulation_reducer::action::perp::{
        ChangeMarginModeAction, ChangeMarginModeLiveInputs, PerpAction,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::position::MarginMode;
    use simulation_state::primitives::U256;

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_market, sample_venue,
    };

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = ChangeMarginModeAction {
            venue: sample_venue(),
            market: sample_market(),
            new_mode: MarginMode::Isolated,
            live_inputs: ChangeMarginModeLiveInputs {
                affected_positions: live(vec!["pos-1".to_owned()]),
                margin_reallocation: live(vec![("pos-1".to_owned(), U256::from(2_000_000_000u64))]),
            },
        };
        (
            ActionBody::Perp(PerpAction::ChangeMarginMode(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn change_margin_mode_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("change_margin_mode", &body, &meta);
    }
}
