//! `HyperliquidCore::HlUnknown` lowering → `Core::UnknownContext`.
//!
//! HL `/exchange` actions with no explicit model degrade to the generic
//! `Core::Action::"Unknown"` so the unknown-blind-sign default warns on them. An
//! off-chain HL action has no EVM target/calldata, so those fields are
//! sentinels; `chain` is the HL sentinel CAIP-2 id.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUnknownAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// CAIP-2 sentinel for the off-chain Hyperliquid L1.
pub(super) const HL_SENTINEL_CHAIN: &str = "hyperliquid:mainnet";
/// Zero address sentinel for an off-chain HL action with no EVM target.
pub(super) const HL_ZERO_ADDR: &str = "0x0000000000000000000000000000000000000000";

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    _action: &HlUnknownAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("target".into(), Value::String(HL_ZERO_ADDR.into()));
    m.insert("chain".into(), Value::String(HL_SENTINEL_CHAIN.into()));
    m.insert("calldata".into(), Value::String("0x".into()));
    m.insert("value".into(), Value::String("0x0".into()));

    Ok(ctx.lowered(r#"Core::Action::"Unknown""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_transition::action::hyperliquid_core::{HlUnknownAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn unknown_lowering_conforms_to_core_unknown_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::Unknown(HlUnknownAction {
            action_type: "convertToMultiSigUser".to_owned(),
        }));
        // Phase 0.2: the HL action's OWN tag. After the RESOLVER_TABLE repoint of
        // ("hyperliquid_core","hl_unknown") → CORE_UNKNOWN_SCHEMA, this composes
        // the Core::Unknown schema and the lowered uid validates.
        assert_conforms("hl_unknown", &body, &offchain_meta());
    }
}
