//! `HyperliquidCore::HlApproveBuilderFee` lowering →
//! `HyperliquidCore::HlApproveBuilderFeeContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::hyperliquid_core::HlApproveBuilderFeeAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlApproveBuilderFeeAction` into the
/// `HyperliquidCore::HlApproveBuilderFeeContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlApproveBuilderFeeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "maxFeeRate".into(),
        Value::String(action.max_fee_rate.clone()),
    );
    m.insert("builder".into(), Value::String(addr(&action.builder)));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlApproveBuilderFee""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::hyperliquid_core::{
        HlApproveBuilderFeeAction, HyperliquidCoreAction,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::Address;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn approve_builder_fee_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::ApproveBuilderFee(
            HlApproveBuilderFeeAction {
                max_fee_rate: "0.001%".to_owned(),
                builder: Address::from_str("0x000000000000000000000000000000000000bEEF").unwrap(),
            },
        ));
        assert_conforms("hl_approve_builder_fee", &body, &offchain_meta());
    }
}
