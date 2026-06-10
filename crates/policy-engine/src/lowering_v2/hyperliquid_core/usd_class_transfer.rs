//! `HyperliquidCore::HlUsdClassTransfer` lowering → `Core::UnknownContext`.
//!
//! `usdClassTransfer` shifts USDC between the user's Hyperliquid spot and perp
//! sub-wallets — a benign intra-account move (no external destination). It is
//! not worth a dedicated Cedar schema, so it degrades to the generic
//! `Core::Action::"Unknown"` (the unknown-blind-sign default warns on it), the
//! same routing used for `hl_send_to_evm_with_data` / `hl_unknown`. The HL
//! decode (both the `/exchange` and EIP-712 typed-sig paths) is unchanged; only
//! the lowering target moves. An off-chain HL action has no EVM target/calldata,
//! so those fields are sentinels.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlUsdClassTransferAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::unknown::{HL_SENTINEL_CHAIN, HL_ZERO_ADDR};

/// Lower an `HlUsdClassTransferAction` into the generic `Core::UnknownContext`.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    _action: &HlUsdClassTransferAction,
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
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{
        HlUsdClassTransferAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn usd_class_transfer_lowers_to_core_unknown() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::UsdClassTransfer(
            HlUsdClassTransferAction {
                amount: Decimal::new("100.5"),
                to_perp: true,
            },
        ));
        // The `hl_usd_class_transfer` trigger now composes the Core::Unknown schema.
        assert_conforms("hl_usd_class_transfer", &body, &offchain_meta());
    }
}
