//! `LiquidStaking::ClaimWithdrawal` lowering → `LiquidStaking::ClaimWithdrawalContext`.

use serde_json::{Map, Value};

use policy_transition::action::liquid_staking::ClaimWithdrawalAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::ClaimWithdrawal` action. No live inputs. `requestIds`
/// is a `Set<String>` of U256-hex withdrawal-request ids.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &ClaimWithdrawalAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_staking_venue(&action.venue));
    m.insert(
        "requestIds".into(),
        Value::Array(
            action
                .request_ids
                .iter()
                .map(|id| Value::String(u256_hex(*id)))
                .collect(),
        ),
    );
    if let Some(recipient) = &action.recipient {
        m.insert("recipient".into(), Value::String(addr(recipient)));
    }

    Ok(ctx.lowered(
        r#"LiquidStaking::Action::"ClaimWithdrawal""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::liquid_staking::{ClaimWithdrawalAction, LiquidStakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{lido_venue, onchain_meta, other};

    fn body(recipient: bool) -> ActionBody {
        ActionBody::LiquidStaking(LiquidStakingAction::ClaimWithdrawal(
            ClaimWithdrawalAction {
                venue: lido_venue(),
                request_ids: vec![U256::from(42u64), U256::from(43u64)],
                recipient: if recipient { Some(other()) } else { None },
            },
        ))
    }

    #[test]
    fn claim_to_recipient_conforms() {
        super::super::test_support::assert_conforms(
            "claim_withdrawal",
            &body(true),
            &onchain_meta(),
        );
    }

    #[test]
    fn claim_no_recipient_conforms() {
        super::super::test_support::assert_conforms(
            "claim_withdrawal",
            &body(false),
            &onchain_meta(),
        );
    }
}
