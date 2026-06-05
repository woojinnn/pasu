//! `Governance::Propose` lowering.

use serde_json::{Map, Value};

use policy_transition::action::governance::GovernanceProposeAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_governance_venue;

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GovernanceProposeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_governance_venue(&action.venue));
    m.insert(
        "payloadTargets".into(),
        Value::Array(
            action
                .payload_targets
                .iter()
                .map(|a| Value::String(addr(a)))
                .collect(),
        ),
    );
    m.insert(
        "payloadCount".into(),
        Value::String(u256_hex(action.payload_count)),
    );
    m.insert("metadata".into(), Value::String(action.metadata.clone()));

    Ok(ctx.lowered(r#"Governance::Action::"Propose""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::governance::{GovernanceAction, GovernanceProposeAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{aave_governance_v3, assert_conforms, onchain_meta, other};

    #[test]
    fn propose_lowering_conforms() {
        let body = ActionBody::Governance(GovernanceAction::Propose(GovernanceProposeAction {
            venue: aave_governance_v3(),
            payload_targets: vec![other()],
            payload_count: U256::from(1u8),
            metadata: "0x1234".into(),
        }));
        assert_conforms("propose", &body, &onchain_meta());
    }
}
