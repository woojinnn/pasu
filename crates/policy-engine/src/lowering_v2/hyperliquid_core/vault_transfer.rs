//! `HyperliquidCore::HlVaultTransfer` lowering →
//! `HyperliquidCore::HlVaultTransferContext`.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlVaultTransferAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::hl_venue;

/// Lower an `HlVaultTransferAction` into the
/// `HyperliquidCore::HlVaultTransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlVaultTransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_venue());
    m.insert(
        "vaultAddress".into(),
        Value::String(addr(&action.vault_address)),
    );
    m.insert("isDeposit".into(), Value::Bool(action.is_deposit));
    m.insert("usd".into(), Value::String(action.usd.0.clone()));

    Ok(ctx.lowered(
        r#"HyperliquidCore::Action::"HlVaultTransfer""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, Decimal};
    use policy_transition::action::hyperliquid_core::{
        HlVaultTransferAction, HyperliquidCoreAction,
    };
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn vault_transfer_lowering_conforms_to_schema() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::VaultTransfer(
            HlVaultTransferAction {
                vault_address: Address::from_str("0x000000000000000000000000000000000000dEaD")
                    .unwrap(),
                is_deposit: true,
                usd: Decimal::new("250"),
            },
        ));
        assert_conforms("hl_vault_transfer", &body, &offchain_meta());
    }
}
