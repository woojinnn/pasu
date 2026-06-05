//! `HyperliquidCore::HlTokenDelegate` lowering →
//! `HyperliquidCore::HlTokenDelegateContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlTokenDelegateAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlTokenDelegateAction` into the
/// `HyperliquidCore::HlTokenDelegateContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlTokenDelegateAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert("validator".into(), Value::String(addr(&action.validator)));
    m.insert("isUndelegate".into(), Value::Bool(action.is_undelegate));
    m.insert("wei".into(), Value::String(action.wei.0.clone()));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlTokenDelegate""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, Decimal};
    use policy_transition::action::hyperliquid_core::{
        HlTokenDelegateAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn token_delegate_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::TokenDelegate(
            HlTokenDelegateAction {
                validator: Address::from_str("0x000000000000000000000000000000000000bEEF").unwrap(),
                is_undelegate: false,
                wei: Decimal::new("1000000000"),
            },
        ));
        assert_conforms("hl_token_delegate", &body, &offchain_meta());
    }
}
