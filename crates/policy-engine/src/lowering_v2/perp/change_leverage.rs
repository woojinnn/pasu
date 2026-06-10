//! `Perp::ChangeLeverage` lowering → `Perp::ChangeLeverageContext`.

use serde_json::{Map, Value};

use policy_transition::action::perp::ChangeLeverageAction;

use crate::cedar_json::decimal_json;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_market_ref, lower_perp_venue};

/// Lower a `ChangeLeverageAction` into the `Perp::ChangeLeverageContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ChangeLeverageAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    m.insert("market".into(), lower_market_ref(&action.market));
    // `newLeverage` is a Cedar `decimal` (leverage fits well within Cedar's
    // 4-dp precision, unlike prices), so a policy can compare it directly via
    // `.greaterThan(decimal("20.0"))`. Normalized to a Cedar-valid decimal arg.
    m.insert(
        "newLeverage".into(),
        decimal_json(&cedar_decimal_arg(&action.new_leverage.0)),
    );
    // ChangeLeverageLiveInputs flattened — emitted only when present
    // (on-chain). Omitted for the Hyperliquid pre-sign path.
    if let Some(li) = &action.live_inputs {
        m.insert(
            "maxLeverage".into(),
            Value::String(li.max_leverage.value.0.clone()),
        );
        // `affectedPositions` (Vec<PositionId>) → Set<String>.
        let affected: Vec<Value> = li
            .affected_positions
            .value
            .iter()
            .map(|id| Value::String(id.clone()))
            .collect();
        m.insert("affectedPositions".into(), Value::Array(affected));
        // `newLiqPrices` (Vec<(PositionId, Option<Price>)>) →
        // Set<{ positionId, liqPrice? }>. `liqPrice` is omitted when None.
        let liq: Vec<Value> = li
            .new_liq_prices
            .value
            .iter()
            .map(|(id, price)| {
                let mut e = Map::new();
                e.insert("positionId".into(), Value::String(id.clone()));
                if let Some(price) = price {
                    e.insert("liqPrice".into(), Value::String(price.0.clone()));
                }
                Value::Object(e)
            })
            .collect();
        m.insert("newLiqPrices".into(), Value::Array(liq));
    }
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"ChangeLeverage""#, Value::Object(m)))
}

/// Normalize a decimal string to a Cedar-`decimal`-valid arg: ensure a decimal
/// point with 1–4 fractional digits (Cedar `decimal()` rejects a bare integer
/// and values with more than 4 fractional digits). `"25"` → `"25.0"`,
/// `"12.5"` → `"12.5"`, `"10.00000"` → `"10.0000"`. Leverage never exceeds a few
/// decimal places, so the 4-dp truncation is lossless in practice.
fn cedar_decimal_arg(s: &str) -> String {
    let s = s.trim();
    let (int_part, frac) = s.split_once('.').unwrap_or((s, ""));
    let int_part = if int_part.is_empty() { "0" } else { int_part };
    let frac: String = frac.chars().take(4).collect();
    let frac = if frac.is_empty() {
        "0".to_owned()
    } else {
        frac
    };
    format!("{int_part}.{frac}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::{Decimal, Price};
    use policy_transition::action::perp::{
        ChangeLeverageAction, ChangeLeverageLiveInputs, PerpAction,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_market, sample_venue,
    };

    fn sample() -> (ActionBody, policy_transition::action::ActionMeta) {
        let action = ChangeLeverageAction {
            venue: sample_venue(),
            market: sample_market(),
            new_leverage: Decimal::new("10"),
            live_inputs: Some(ChangeLeverageLiveInputs {
                max_leverage: live(Decimal::new("20")),
                affected_positions: live(vec!["pos-1".to_owned(), "pos-2".to_owned()]),
                // Exercise both Some and None liqPrice arms.
                new_liq_prices: live(vec![
                    ("pos-1".to_owned(), Some(Price::new("2500"))),
                    ("pos-2".to_owned(), None),
                ]),
            }),
        };
        (
            ActionBody::Perp(PerpAction::ChangeLeverage(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn change_leverage_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("change_leverage", &body, &meta);
    }

    /// Hyperliquid pre-sign shape: `live_inputs: None` — the live fields are
    /// omitted and the context still conforms (they are optional).
    #[test]
    fn change_leverage_hl_shape_no_live_inputs_conforms() {
        let action = ChangeLeverageAction {
            venue: sample_venue(),
            market: sample_market(),
            new_leverage: Decimal::new("10"),
            live_inputs: None,
        };
        let body = ActionBody::Perp(PerpAction::ChangeLeverage(action));
        assert_conforms("change_leverage", &body, &onchain_meta());
    }

    /// `newLeverage` lowers to a Cedar `decimal` extension value (so a policy
    /// can `.greaterThan(decimal("20.0"))`), and an integral HL leverage gets a
    /// Cedar-valid arg (`"25"` → `"25.0"`).
    #[test]
    fn change_leverage_emits_cedar_decimal() {
        use crate::lowering_v2::{lower_action, TxMeta};
        let tx = TxMeta {
            from: super::super::test_support::FROM,
            to: super::super::test_support::TO,
        };
        let action = ChangeLeverageAction {
            venue: sample_venue(),
            market: sample_market(),
            new_leverage: Decimal::new("25"),
            live_inputs: None,
        };
        let body = ActionBody::Perp(PerpAction::ChangeLeverage(action));
        let ctx = lower_action(&body, &onchain_meta(), &tx).unwrap().context;
        assert_eq!(
            ctx["newLeverage"],
            serde_json::json!({ "__extn": { "fn": "decimal", "arg": "25.0" } })
        );
    }

    #[test]
    fn cedar_decimal_arg_normalizes() {
        assert_eq!(super::cedar_decimal_arg("25"), "25.0");
        assert_eq!(super::cedar_decimal_arg("25.0"), "25.0");
        assert_eq!(super::cedar_decimal_arg("12.5"), "12.5");
        assert_eq!(super::cedar_decimal_arg("10.00000"), "10.0000");
    }
}
