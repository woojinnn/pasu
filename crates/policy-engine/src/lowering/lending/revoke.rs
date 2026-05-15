use crate::action::lending::RevokeAction;
use crate::context_keys::{CALLER, REVOKE_KIND, SUBJECT, TARGET};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{contract_ref_json, revoke_kind_str};

const ACTION_ID: &str = "revoke";

impl Lower for RevokeAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(target) = &self.target {
            context.insert(TARGET.into(), contract_ref_json(target));
        }
        context.insert(CALLER.into(), Value::from(self.caller.to_string()));
        context.insert(SUBJECT.into(), Value::from(self.subject.to_string()));
        context.insert(
            REVOKE_KIND.into(),
            Value::from(revoke_kind_str(&self.revoke_kind)),
        );

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::lending::{RevokeAction, RevokeKind};
    use crate::action::Action;
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, contract_ref, envelope, policy_request,
    };

    fn revoke(
        caller: crate::action::Address,
        subject: crate::action::Address,
    ) -> RevokeAction {
        RevokeAction {
            target: None,
            caller,
            subject,
            revoke_kind: RevokeKind::Erc20Allowance,
        }
    }

    #[test]
    fn revoke_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let subject = address("0x3333333333333333333333333333333333333333");
        let request = policy_request(
            &envelope(Action::Revoke(revoke(from.clone(), subject.clone()))),
            &from,
        );

        assert!(request.action.contains("revoke"));
        assert_eq!(
            request.context.get("caller").and_then(Value::as_str),
            Some(from.to_string().as_str())
        );
        assert_eq!(
            request.context.get("subject").and_then(Value::as_str),
            Some(subject.to_string().as_str())
        );
        assert_eq!(
            request.context.get("revokeKind").and_then(Value::as_str),
            Some("erc20_allowance")
        );
        assert!(request.context.get("target").is_none());
    }

    #[test]
    fn revoke_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let subject = address("0x3333333333333333333333333333333333333333");
        let mut action = revoke(from.clone(), subject);
        action.target = Some(contract_ref());
        action.revoke_kind = RevokeKind::PositionManagerRole;

        let request = policy_request(&envelope(Action::Revoke(action)), &from);

        assert_eq!(
            request.context.get("revokeKind").and_then(Value::as_str),
            Some("position_manager_role")
        );
        assert!(request.context.get("target").is_some());
        assert_eq!(
            request
                .context
                .get("target")
                .and_then(|target| target.get("label"))
                .and_then(Value::as_str),
            Some("Pool")
        );
    }
}
