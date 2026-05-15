use crate::action::restaking::RequestRestakeWithdrawalAction;
use crate::context_keys::{AMOUNT_IN, AMOUNT_OUT, RECEIPT_TOKEN, RECIPIENT, STRATEGY, TOKEN_OUT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::restaking::common::strategy_json;

const ACTION_ID: &str = "request_restake_withdrawal";

// `RequestRestakeWithdrawalAction.ticket` has no counterpart in
// `policy-schema/actions/restaking/request_restake_withdrawal.cedarschema`.
// Cedar rejects extra context fields against typed schemas, so the lowering
// omits `ticket` here. Adding it requires extending the schema first —
// flagged as a follow-up in the PR body.

impl Lower for RequestRestakeWithdrawalAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(token_out) = &self.token_out {
            context.insert(TOKEN_OUT.into(), asset_ref_json(token_out));
        }
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
    use crate::action::restaking::RequestRestakeWithdrawalAction;
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::restaking::test_support::{
        address, amount, empty_ticket, envelope, erc20, native, policy_request, strategy,
    };

    fn request_restake_withdrawal(
        recipient: crate::action::Address,
    ) -> RequestRestakeWithdrawalAction {
        RequestRestakeWithdrawalAction {
            token_out: None,
            receipt_token: None,
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: None,
            strategy: None,
            ticket: None,
            recipient,
        }
    }

    #[test]
    fn request_restake_withdrawal_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::RequestRestakeWithdrawal(
                request_restake_withdrawal(from.clone()),
            )),
            &from,
        );

        assert!(request.action.contains("request_restake_withdrawal"));
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
        assert!(request.context.get("tokenOut").is_none());
        assert!(request.context.get("receiptToken").is_none());
        assert!(request.context.get("amountOut").is_none());
        assert!(request.context.get("strategy").is_none());
        // `ticket` is intentionally omitted — not declared in the schema.
        assert!(request.context.get("ticket").is_none());
    }

    #[test]
    fn request_restake_withdrawal_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut action = request_restake_withdrawal(from.clone());
        action.token_out = Some(native("ETH"));
        action.receipt_token = Some(erc20(
            "0xbf5495efe5db9ce00f80364c8b423567e58d2110",
            "ezETH",
            18,
        ));
        action.amount_out = Some(amount(AmountKind::Estimated, "999000000000000000"));
        action.strategy = Some(strategy());
        // Ticket is set but the lowering must not surface it in the context.
        action.ticket = Some(empty_ticket());

        let request = policy_request(&envelope(Action::RequestRestakeWithdrawal(action)), &from);

        assert_eq!(
            request
                .context
                .get("tokenOut")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("ETH")
        );
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
                .get("strategy")
                .and_then(|strategy| strategy.get("label"))
                .and_then(Value::as_str),
            Some("EigenLayer ezETH")
        );
        // Setting a ticket on the action does not leak into the Cedar context.
        assert!(request.context.get("ticket").is_none());
    }
}
