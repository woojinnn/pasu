//! `Governance::UpdateRepresentative` lowering.

use serde_json::{Map, Value};

use policy_transition::action::governance::GovernanceUpdateRepresentativeAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_governance_venue;

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GovernanceUpdateRepresentativeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_governance_venue(&action.venue));
    m.insert(
        "representative".into(),
        Value::String(addr(&action.representative)),
    );
    m.insert(
        "representativeChainId".into(),
        Value::String(u256_hex(action.representative_chain_id)),
    );

    Ok(ctx.lowered(
        r#"Governance::Action::"UpdateRepresentative""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::governance::{
        GovernanceAction, GovernanceUpdateRepresentativeAction,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{aave_governance_v3, assert_conforms, onchain_meta, other};

    #[test]
    fn update_representative_lowering_conforms() {
        let body = ActionBody::Governance(GovernanceAction::UpdateRepresentative(
            GovernanceUpdateRepresentativeAction {
                venue: aave_governance_v3(),
                representative: other(),
                representative_chain_id: U256::from(1u8),
            },
        ));
        assert_conforms("update_representative", &body, &onchain_meta());
    }
}
