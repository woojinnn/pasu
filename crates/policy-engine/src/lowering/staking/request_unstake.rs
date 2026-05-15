use crate::action::staking::RequestUnstakeAction;
use crate::context_keys::{AMOUNT_IN, AMOUNT_OUT, RECEIPT_TOKEN, RECIPIENT, TOKEN_OUT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "request_unstake";

// `RequestUnstakeAction.ticket` has no counterpart in
// `policy-schema/actions/staking/request_unstake.cedarschema`. Cedar rejects
// extra context fields against typed schemas, so the lowering omits `ticket`
// here. Adding it requires extending the schema first — flagged as a follow-up
// in the PR body.

impl Lower for RequestUnstakeAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        context.insert(RECEIPT_TOKEN.into(), asset_ref_json(&self.receipt_token));
        if let Some(token_out) = &self.token_out {
            context.insert(TOKEN_OUT.into(), asset_ref_json(token_out));
        }
        context.insert(AMOUNT_IN.into(), amount_constraint_json(&self.amount_in));
        if let Some(amount_out) = &self.amount_out {
            context.insert(AMOUNT_OUT.into(), amount_constraint_json(amount_out));
        }
        context.insert(RECIPIENT.into(), Value::from(self.recipient.to_string()));

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::staking::RequestUnstakeAction;
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::staking::test_support::{
        address, amount, empty_ticket, envelope, erc20, native, policy_request,
    };

    fn request_unstake(recipient: crate::action::Address) -> RequestUnstakeAction {
        RequestUnstakeAction {
            receipt_token: erc20("0xae7ab96520de3a18e5e111b5eaab095312d7fe84", "stETH", 18),
            token_out: None,
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: None,
            ticket: None,
            recipient,
        }
    }

    #[test]
    fn request_unstake_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::RequestUnstake(request_unstake(from.clone()))),
            &from,
        );

        assert!(request.action.contains("request_unstake"));
        assert_eq!(
            request
                .context
                .get("receiptToken")
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
        assert!(request.context.get("tokenOut").is_none());
        assert!(request.context.get("amountOut").is_none());
        // `ticket` is intentionally omitted — not declared in the schema.
        assert!(request.context.get("ticket").is_none());
    }

    #[test]
    fn request_unstake_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut action = request_unstake(from.clone());
        action.token_out = Some(native("ETH"));
        action.amount_out = Some(amount(AmountKind::Estimated, "999000000000000000"));
        // Ticket is set but the lowering must not surface it in the context.
        action.ticket = Some(empty_ticket());

        let request = policy_request(&envelope(Action::RequestUnstake(action)), &from);

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
                .get("amountOut")
                .and_then(|amount| amount.get("kind"))
                .and_then(Value::as_str),
            Some("estimated")
        );
        // Setting a ticket on the action does not leak into the Cedar context.
        assert!(request.context.get("ticket").is_none());
    }
}
