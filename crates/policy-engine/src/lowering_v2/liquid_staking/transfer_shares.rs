//! `LiquidStaking::TransferShares` lowering → `LiquidStaking::TransferSharesContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::liquid_staking::TransferSharesAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::TransferShares` action. No live inputs. `shares` is
/// the protocol's internal share unit (not stETH balance).
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &TransferSharesAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_staking_venue(&action.venue));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert("shares".into(), Value::String(u256_hex(action.shares)));
    if let Some(from) = &action.from {
        m.insert("from".into(), Value::String(addr(from)));
    }

    Ok(ctx.lowered(
        r#"LiquidStaking::Action::"TransferShares""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::liquid_staking::{LiquidStakingAction, TransferSharesAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;

    use super::super::test_support::{lido_venue, onchain_meta, other};

    fn body(from: bool) -> ActionBody {
        ActionBody::LiquidStaking(LiquidStakingAction::TransferShares(TransferSharesAction {
            venue: lido_venue(),
            recipient: other(),
            shares: U256::from(123_456u64),
            from: if from { Some(other()) } else { None },
        }))
    }

    #[test]
    fn transfer_shares_direct_conforms() {
        super::super::test_support::assert_conforms(
            "transfer_shares",
            &body(false),
            &onchain_meta(),
        );
    }

    #[test]
    fn transfer_shares_from_conforms() {
        super::super::test_support::assert_conforms(
            "transfer_shares",
            &body(true),
            &onchain_meta(),
        );
    }
}
