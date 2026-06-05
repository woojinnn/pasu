//! `HyperliquidCore::HlApproveAgent` lowering →
//! `HyperliquidCore::HlApproveAgentContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlApproveAgentAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlApproveAgentAction` into the
/// `HyperliquidCore::HlApproveAgentContext` shape. `agentName` is omitted when
/// absent.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlApproveAgentAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "agentAddress".into(),
        Value::String(addr(&action.agent_address)),
    );
    if let Some(name) = &action.agent_name {
        m.insert("agentName".into(), Value::String(name.clone()));
    }

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlApproveAgent""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::Address;
    use policy_transition::action::hyperliquid_core::{
        HlApproveAgentAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn approve_agent_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::ApproveAgent(
            HlApproveAgentAction {
                agent_address: Address::from_str("0x00000000000000000000000000000000000a6e47")
                    .unwrap(),
                agent_name: Some("trading-bot".to_owned()),
            },
        ));
        assert_conforms("hl_approve_agent", &body, &offchain_meta());
    }

    /// Agent with no name still conforms (agentName omitted).
    #[test]
    fn approve_agent_without_name_conforms() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::ApproveAgent(
            HlApproveAgentAction {
                agent_address: Address::from_str("0x00000000000000000000000000000000000a6e47")
                    .unwrap(),
                agent_name: None,
            },
        ));
        assert_conforms("hl_approve_agent", &body, &offchain_meta());
    }
}
