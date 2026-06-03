//! `Governance::Delegate` lowering.

use serde_json::{Map, Value};

use policy_transition::action::governance::{GovernanceDelegateAction, GovernanceDelegationKind};

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_governance_venue;

const fn kind_name(kind: &GovernanceDelegationKind) -> &'static str {
    match kind {
        GovernanceDelegationKind::All => "all",
        GovernanceDelegationKind::Voting => "voting",
        GovernanceDelegationKind::Proposition => "proposition",
        GovernanceDelegationKind::Raw => "raw",
    }
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GovernanceDelegateAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_governance_venue(&action.venue));
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("delegatee".into(), Value::String(addr(&action.delegatee)));
    m.insert(
        "delegationKind".into(),
        Value::String(kind_name(&action.delegation_kind).into()),
    );
    if let Some(raw) = action.raw_delegation_type {
        m.insert("rawDelegationType".into(), Value::from(i64::from(raw)));
    }
    if let Some(current_delegate) = &action.live_inputs.current_delegate.value {
        m.insert(
            "currentDelegate".into(),
            Value::String(addr(current_delegate)),
        );
    }
    m.insert(
        "governancePower".into(),
        Value::String(u256_hex(action.live_inputs.governance_power.value)),
    );

    Ok(ctx.lowered(r#"Governance::Action::"Delegate""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_state::LiveField;
    use policy_transition::action::governance::{
        GovernanceAction, GovernanceDelegateAction, GovernanceDelegateLiveInputs,
        GovernanceDelegationKind,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        aave_token_ref, assert_conforms, governance_token_venue, now, onchain_meta, onchain_source,
        other,
    };

    #[test]
    fn delegate_lowering_conforms() {
        let body = ActionBody::Governance(GovernanceAction::Delegate(GovernanceDelegateAction {
            venue: governance_token_venue(),
            token: aave_token_ref(),
            delegatee: other(),
            delegation_kind: GovernanceDelegationKind::Voting,
            raw_delegation_type: Some(0),
            live_inputs: GovernanceDelegateLiveInputs {
                current_delegate: LiveField::new(None, onchain_source(), now()),
                governance_power: LiveField::new(U256::from(1_000_000u64), onchain_source(), now()),
            },
        }));
        assert_conforms("delegate", &body, &onchain_meta());
    }
}
