use crate::action::lending::SignAuthorizationAction;
use crate::context_keys::{
    AMOUNT, AUTHORIZATION_SCOPE, AUTHORIZED, AUTHORIZER, IS_AUTHORIZED, MARKET, NONCE,
    VALIDITY_DELTA_SEC,
};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::validity::{validity_delta_sec, validity_json};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{contract_ref_json, sign_authorization_scope_str};

const ACTION_ID: &str = "sign_authorization";
const VALIDITY: &str = "validity";

impl Lower for SignAuthorizationAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(market) = &self.market {
            context.insert(MARKET.into(), contract_ref_json(market));
        }
        context.insert(AUTHORIZER.into(), Value::from(self.authorizer.to_string()));
        context.insert(AUTHORIZED.into(), Value::from(self.authorized.to_string()));
        context.insert(IS_AUTHORIZED.into(), Value::from(self.is_authorized));
        context.insert(
            AUTHORIZATION_SCOPE.into(),
            Value::from(sign_authorization_scope_str(&self.authorization_scope)),
        );
        if let Some(amount) = &self.amount {
            context.insert(AMOUNT.into(), amount_constraint_json(amount));
        }
        if let Some(nonce) = &self.nonce {
            context.insert(NONCE.into(), Value::from(nonce.to_string()));
        }
        context.insert(VALIDITY.into(), validity_json(&self.validity));
        if let Some(delta_sec) = validity_delta_sec(&self.validity, ctx.block_timestamp) {
            context.insert(VALIDITY_DELTA_SEC.into(), Value::from(delta_sec));
        }

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::lending::{SignAuthorizationAction, SignAuthorizationScope};
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, amount, contract_ref, decimal, envelope, policy_request, validity, BLOCK_TIMESTAMP,
    };

    fn sign_authorization(
        authorizer: crate::action::Address,
        authorized: crate::action::Address,
    ) -> SignAuthorizationAction {
        SignAuthorizationAction {
            market: None,
            authorizer,
            authorized,
            is_authorized: true,
            authorization_scope: SignAuthorizationScope::All,
            amount: None,
            nonce: None,
            validity: validity(BLOCK_TIMESTAMP + 600),
        }
    }

    #[test]
    fn sign_authorization_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let authorized = address("0x3333333333333333333333333333333333333333");
        let request = policy_request(
            &envelope(Action::SignAuthorization(sign_authorization(
                from.clone(),
                authorized.clone(),
            ))),
            &from,
        );

        assert!(request.action.contains("sign_authorization"));
        assert_eq!(
            request.context.get("authorizer").and_then(Value::as_str),
            Some(from.to_string().as_str())
        );
        assert_eq!(
            request.context.get("authorized").and_then(Value::as_str),
            Some(authorized.to_string().as_str())
        );
        assert_eq!(
            request
                .context
                .get("authorizationScope")
                .and_then(Value::as_str),
            Some("all")
        );
        assert!(request.context.get("validity").is_some());
        assert_eq!(
            request
                .context
                .get("validityDeltaSec")
                .and_then(Value::as_i64),
            Some(600)
        );
        assert!(request.context.get("market").is_none());
        assert!(request.context.get("amount").is_none());
        assert!(request.context.get("nonce").is_none());
    }

    #[test]
    fn sign_authorization_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let authorized = address("0x3333333333333333333333333333333333333333");
        let mut action = sign_authorization(from.clone(), authorized);
        action.market = Some(contract_ref());
        action.authorization_scope = SignAuthorizationScope::ManagerRole;
        action.amount = Some(amount(AmountKind::Exact, "1000"));
        action.nonce = Some(decimal("7"));

        let request = policy_request(&envelope(Action::SignAuthorization(action)), &from);

        assert_eq!(
            request
                .context
                .get("authorizationScope")
                .and_then(Value::as_str),
            Some("manager_role")
        );
        assert_eq!(
            request.context.get("nonce").and_then(Value::as_str),
            Some("7")
        );
        assert!(request.context.get("market").is_some());
        assert!(request.context.get("amount").is_some());
    }
}
