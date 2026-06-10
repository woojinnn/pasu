//! `HyperliquidCore::HlCDeposit` lowering → `Staking::StakeContext` (HYPE staking).
//!
//! `cDeposit` moves HYPE into the Hyperliquid staking (delegation) balance.
//! Lowers to the generic `Staking::Action::"Stake"` on the HL stake venue;
//! `wei` is already the raw smallest-unit amount, emitted as the `amount` U256
//! hex slot. `amountNano` stays host-populated and is omitted.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HlCDepositAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::amount::hl_amount_projection;
use super::hl_stake_venue;

/// Lower an `HlCDepositAction` into the `Staking::StakeContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &HlCDepositAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    // `wei` is already raw smallest-unit (decimals = 0): hex-encode it for the
    // `amount` String slot (U256 hex everywhere in Staking::Stake).
    let p = hl_amount_projection(&action.wei, 0);
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), hl_stake_venue());
    m.insert("amount".into(), Value::String(p.raw_hex));
    // `asset` / `amountNano` / `onBehalfOf` / `recipient` / `custom` are
    // host-populated or N/A for HL staking — OMITTED here.

    Ok(ctx.lowered(r#"Staking::Action::"Stake""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::doc_markdown)]
mod tests {
    use policy_state::primitives::Decimal;
    use policy_transition::action::hyperliquid_core::{HlCDepositAction, HyperliquidCoreAction};
    use policy_transition::action::ActionBody;

    use crate::lowering_v2::perp::test_support::{assert_conforms, offchain_meta};

    #[test]
    fn c_deposit_lowering_conforms_to_stake() {
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::CDeposit(HlCDepositAction {
            wei: Decimal::new("100000000000"),
        }));
        assert_conforms("stake", &body, &offchain_meta());
    }
}
