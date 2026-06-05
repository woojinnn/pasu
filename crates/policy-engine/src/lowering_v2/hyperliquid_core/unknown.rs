//! `HyperliquidCore::HlUnknown` lowering → `HyperliquidCore::HlUnknownContext`.
//!
//! Catch-all for an `/exchange` action no explicit model matched. Carries only
//! the raw wire `type` string; a policy typically `forbid`s/`warn`s on the
//! `HlUnknown` action UID or scopes on `context.actionType`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUnknownAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlUnknownAction` into the `HyperliquidCore::HlUnknownContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlUnknownAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "actionType".into(),
        Value::String(action.action_type.clone()),
    );

    Ok(ctx.lowered(r#"HyperliquidCore::Action::"HlUnknown""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_transition::action::hyperliquid_core::{HlUnknownAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn unknown_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::Unknown(HlUnknownAction {
            action_type: "convertToMultiSigUser".to_owned(),
        }));
        assert_conforms("hl_unknown", &body, &offchain_meta());
    }
}
