//! `Lending::SetEMode` lowering → `Lending::SetEModeContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::{EModeConfig, SetEModeAction};

use super::super::common::cedar::addr;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, lower_user_lending_state};

/// Lower a `Lending::SetEMode` action into the `Lending::SetEModeContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &SetEModeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert(
        "categoryId".into(),
        Value::from(i64::from(action.category_id)),
    );
    m.insert(
        "categoryConfig".into(),
        lower_emode_config(&action.live_inputs.category_config.value),
    );
    m.insert(
        "userStateBefore".into(),
        lower_user_lending_state(&action.live_inputs.user_state_before.value),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"SetEMode""#, Value::Object(m)))
}

/// Lower an [`EModeConfig`] → `Lending::EModeConfig`. `assetsInCategory` is a
/// `Set<Core::TokenRef>` (a JSON array). `priceSource` / `categoryCode` are
/// omitted when absent.
fn lower_emode_config(config: &EModeConfig) -> Value {
    let mut m = Map::new();
    m.insert("ltvBp".into(), Value::from(i64::from(config.ltv_bp)));
    m.insert(
        "liquidationThresholdBp".into(),
        Value::from(i64::from(config.liquidation_threshold_bp)),
    );
    m.insert(
        "liquidationBonusBp".into(),
        Value::from(i64::from(config.liquidation_bonus_bp)),
    );
    if let Some(price_source) = &config.price_source {
        m.insert("priceSource".into(), Value::String(addr(price_source)));
    }
    let assets: Vec<Value> = config
        .assets_in_category
        .iter()
        .map(lower_token_ref)
        .collect();
    m.insert("assetsInCategory".into(), Value::Array(assets));
    if let Some(category) = config.category {
        // `EModeCategory` is a u8 alias → `categoryCode?: Long`.
        m.insert("categoryCode".into(), Value::from(i64::from(category)));
    }
    Value::Object(m)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::lending::{
        EModeConfig, LendingAction, SetEModeAction, SetEModeLiveInputs,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::Address;

    use super::super::test_support::{live, onchain_meta, user_state, usdc, venue, weth};

    /// A representative e-mode selection (category 1) with a price source and
    /// two eligible assets — exercises `priceSource` PRESENT, `categoryCode`
    /// PRESENT, and a non-empty `assetsInCategory`.
    #[test]
    fn set_e_mode_lowering_conforms_to_schema() {
        let action = LendingAction::SetEMode(SetEModeAction {
            venue: venue(),
            category_id: 1,
            live_inputs: SetEModeLiveInputs {
                category_config: live(EModeConfig {
                    ltv_bp: 9300,
                    liquidation_threshold_bp: 9500,
                    liquidation_bonus_bp: 100,
                    price_source: Some(
                        Address::from_str("0x000000000000000000000000000000000000c03e").unwrap(),
                    ),
                    assets_in_category: vec![usdc(), weth()],
                    category: Some(1),
                }),
                user_state_before: live(user_state()),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("set_e_mode", &body, &meta);
    }

    /// E-mode DISABLE (`category_id == 0`) with `price_source == None`,
    /// `category == None`, and an EMPTY `assets_in_category` — exercises the
    /// omitted-`priceSource`, omitted-`categoryCode`, and empty-Set branches of
    /// `lower_emode_config`.
    #[test]
    fn set_e_mode_disable_without_optionals_conforms() {
        let action = LendingAction::SetEMode(SetEModeAction {
            venue: venue(),
            category_id: 0,
            live_inputs: SetEModeLiveInputs {
                category_config: live(EModeConfig {
                    ltv_bp: 0,
                    liquidation_threshold_bp: 0,
                    liquidation_bonus_bp: 0,
                    price_source: None,
                    assets_in_category: vec![],
                    category: None,
                }),
                user_state_before: live(user_state()),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("set_e_mode", &body, &meta);
    }
}
