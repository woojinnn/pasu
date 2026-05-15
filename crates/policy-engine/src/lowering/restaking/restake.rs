use crate::action::restaking::RestakeAction;
use crate::context_keys::{AMOUNT_IN, AMOUNT_OUT, RECEIPT_TOKEN, RECIPIENT, STRATEGY, TOKEN_IN};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::restaking::common::strategy_json;

const ACTION_ID: &str = "restake";

impl Lower for RestakeAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        context.insert(TOKEN_IN.into(), asset_ref_json(&self.token_in));
        if let Some(receipt_token) = &self.receipt_token {
            context.insert(RECEIPT_TOKEN.into(), asset_ref_json(receipt_token));
        }
        context.insert(AMOUNT_IN.into(), amount_constraint_json(&self.amount_in));
        if let Some(amount_out) = &self.amount_out {
            context.insert(AMOUNT_OUT.into(), amount_constraint_json(amount_out));
        }
        if let Some(strategy) = &self.strategy {
            context.insert(STRATEGY.into(), strategy_json(strategy));
        }
        context.insert(RECIPIENT.into(), Value::from(self.recipient.to_string()));

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::restaking::RestakeAction;
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::restaking::test_support::{
        address, amount, envelope, erc20, policy_request, strategy,
    };

    fn restake(recipient: crate::action::Address) -> RestakeAction {
        RestakeAction {
            token_in: erc20("0xae7ab96520de3a18e5e111b5eaab095312d7fe84", "stETH", 18),
            receipt_token: None,
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: None,
            strategy: None,
            recipient,
        }
    }

    #[test]
    fn restake_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(&envelope(Action::Restake(restake(from.clone()))), &from);

        assert_eq!(
            request.principal,
            r#"Wallet::"0x1111111111111111111111111111111111111111""#
        );
        assert!(request.action.contains("restake"));
        assert_eq!(
            request.resource,
            r#"Protocol::"0x2222222222222222222222222222222222222222""#
        );
        assert_eq!(
            request
                .context
                .get("tokenIn")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("stETH")
        );
        assert_eq!(
            request
                .context
                .get("amountIn")
                .and_then(|amount| amount.get("value"))
                .and_then(Value::as_str),
            Some("1000000000000000000")
        );
        assert_eq!(
            request.context.get("recipient").and_then(Value::as_str),
            Some(from.to_string().as_str())
        );
        assert!(request.context.get("receiptToken").is_none());
        assert!(request.context.get("amountOut").is_none());
        assert!(request.context.get("strategy").is_none());
    }

    #[test]
    fn restake_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut action = restake(from.clone());
        action.receipt_token = Some(erc20(
            "0xbf5495efe5db9ce00f80364c8b423567e58d2110",
            "ezETH",
            18,
        ));
        action.amount_out = Some(amount(AmountKind::Estimated, "999000000000000000"));
        action.strategy = Some(strategy());

        let request = policy_request(&envelope(Action::Restake(action)), &from);

        assert_eq!(
            request
                .context
                .get("receiptToken")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("ezETH")
        );
        assert_eq!(
            request
                .context
                .get("amountOut")
                .and_then(|amount| amount.get("kind"))
                .and_then(Value::as_str),
            Some("estimated")
        );
        assert_eq!(
            request
                .context
                .get("strategy")
                .and_then(|strategy| strategy.get("label"))
                .and_then(Value::as_str),
            Some("EigenLayer ezETH")
        );
    }
}
