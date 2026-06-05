//! `LiquidStaking::TransferShares` lowering → `LiquidStaking::TransferSharesContext`.

use serde_json::{Map, Value};

use policy_transition::action::liquid_staking::TransferSharesAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::TransferShares` action. `shares` is the protocol's
/// internal share unit (not stETH balance); `pooledEth` is the host-populated
/// live field — the stETH amount those shares correspond to
/// (`getPooledEthByShares(shares)`), so the user sees what the recipient gets.
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
    m.insert(
        "pooledEth".into(),
        Value::String(u256_hex(action.live_inputs.pooled_eth.value)),
    );
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
    use policy_state::primitives::U256;
    use policy_transition::action::liquid_staking::{
        LiquidStakingAction, TransferSharesAction, TransferSharesLiveInputs,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{lido_venue, live_u256, onchain_meta, other};

    fn body(from: bool) -> ActionBody {
        ActionBody::LiquidStaking(LiquidStakingAction::TransferShares(TransferSharesAction {
            venue: lido_venue(),
            recipient: other(),
            shares: U256::from(123_456u64),
            from: if from { Some(other()) } else { None },
            live_inputs: TransferSharesLiveInputs {
                pooled_eth: live_u256(),
            },
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
