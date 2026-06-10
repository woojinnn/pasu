//! `HyperliquidCore::HlCWithdraw` lowering → `Staking::RedeemContext` (HYPE unstake).
//!
//! `cWithdraw` pulls HYPE out of the Hyperliquid staking balance back to spot.
//! Lowers to the generic `Staking::Action::"Redeem"` on the HL stake venue;
//! `wei` is already the raw smallest-unit amount, emitted as the `amount` U256
//! hex slot.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlCWithdrawAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::amount::hl_amount_projection;
use super::hl_stake_venue;

/// Lower an `HlCWithdrawAction` into the `Staking::RedeemContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlCWithdrawAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let p = hl_amount_projection(&action.wei, 0);
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_stake_venue());
    m.insert("amount".into(), Value::String(p.raw_hex));
    // `recipient` / `custom` are host-populated or N/A — OMITTED here.

    Ok(ctx.lowered(r#"Staking::Action::"Redeem""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{HlCWithdrawAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn c_withdraw_lowering_conforms_to_redeem() {
        let body =
            ActionBody::HyperliquidCore(HyperliquidCoreAction::CWithdraw(HlCWithdrawAction {
                wei: Decimal::new("100000000000"),
            }));
        assert_conforms("redeem", &body, &offchain_meta());
    }
}
