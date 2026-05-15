use crate::action::lending::SetAuthorizationAction;
use crate::context_keys::{
    AMOUNT, AUTHORIZATION_SCOPE, AUTHORIZED, AUTHORIZER, IS_AUTHORIZED, MARKET,
};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{authorization_scope_str, market_json};

const ACTION_ID: &str = "set_authorization";

impl Lower for SetAuthorizationAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(market) = &self.market {
            context.insert(MARKET.into(), market_json(market));
        }
        context.insert(AUTHORIZER.into(), Value::from(self.authorizer.to_string()));
        context.insert(AUTHORIZED.into(), Value::from(self.authorized.to_string()));
        context.insert(IS_AUTHORIZED.into(), Value::from(self.is_authorized));
        context.insert(
            AUTHORIZATION_SCOPE.into(),
            Value::from(authorization_scope_str(&self.authorization_scope)),
        );
        if let Some(amount) = &self.amount {
            context.insert(AMOUNT.into(), amount_constraint_json(amount));
        }

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::lending::{AuthorizationScope, SetAuthorizationAction};
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, amount, envelope, market, policy_request,
    };

    fn set_authorization(
        authorizer: crate::action::Address,
        authorized: crate::action::Address,
    ) -> SetAuthorizationAction {
        SetAuthorizationAction {
            market: None,
            authorizer,
            authorized,
            is_authorized: true,
            authorization_scope: AuthorizationScope::All,
            amount: None,
        }
    }

    #[test]
    fn set_authorization_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let authorized = address("0x3333333333333333333333333333333333333333");
        let request = policy_request(
            &envelope(Action::SetAuthorization(set_authorization(
                from.clone(),
                authorized.clone(),
            ))),
            &from,
        );

        assert!(request.action.contains("set_authorization"));
        assert_eq!(
            request.context.get("authorizer").and_then(Value::as_str),
            Some(from.to_string().as_str())
        );
        assert_eq!(
            request.context.get("authorized").and_then(Value::as_str),
            Some(authorized.to_string().as_str())
        );
        assert_eq!(
            request.context.get("isAuthorized").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            request
                .context
                .get("authorizationScope")
                .and_then(Value::as_str),
            Some("all")
        );
        assert!(request.context.get("market").is_none());
        assert!(request.context.get("amount").is_none());
    }

    #[test]
    fn set_authorization_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let authorized = address("0x3333333333333333333333333333333333333333");
        let mut action = set_authorization(from.clone(), authorized);
        action.market = Some(market());
        action.is_authorized = false;
        action.authorization_scope = AuthorizationScope::PositionManagerRole;
        action.amount = Some(amount(AmountKind::Unlimited, "0"));

        let request = policy_request(&envelope(Action::SetAuthorization(action)), &from);

        assert_eq!(
            request.context.get("isAuthorized").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            request
                .context
                .get("authorizationScope")
                .and_then(Value::as_str),
            Some("position_manager_role")
        );
        assert!(request.context.get("market").is_some());
        assert_eq!(
            request
                .context
                .get("amount")
                .and_then(|amount| amount.get("kind"))
                .and_then(Value::as_str),
            Some("unlimited")
        );
    }
}
