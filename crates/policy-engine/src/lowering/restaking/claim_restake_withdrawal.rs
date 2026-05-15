use crate::action::restaking::ClaimRestakeWithdrawalAction;
use crate::context_keys::{AMOUNT_OUT, RECIPIENT, TOKEN_OUT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "claim_restake_withdrawal";

// `ClaimRestakeWithdrawalAction.ticket` (required on the struct) has no
// counterpart in
// `policy-schema/actions/restaking/claim_restake_withdrawal.cedarschema`.
// Cedar rejects extra context fields against typed schemas, so the lowering
// omits `ticket` here. Adding it requires extending the schema first —
// flagged as a follow-up in the PR body.

impl Lower for ClaimRestakeWithdrawalAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        context.insert(TOKEN_OUT.into(), asset_ref_json(&self.token_out));
        if let Some(amount_out) = &self.amount_out {
            context.insert(AMOUNT_OUT.into(), amount_constraint_json(amount_out));
        }
        context.insert(RECIPIENT.into(), Value::from(self.recipient.to_string()));

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::restaking::ClaimRestakeWithdrawalAction;
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::restaking::test_support::{
        address, amount, empty_ticket, envelope, native, policy_request,
    };

    fn claim_restake_withdrawal(recipient: crate::action::Address) -> ClaimRestakeWithdrawalAction {
        ClaimRestakeWithdrawalAction {
            token_out: native("ETH"),
            amount_out: None,
            ticket: empty_ticket(),
            recipient,
        }
    }

    #[test]
    fn claim_restake_withdrawal_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::ClaimRestakeWithdrawal(claim_restake_withdrawal(
                from.clone(),
            ))),
            &from,
        );

        assert!(request.action.contains("claim_restake_withdrawal"));
        assert_eq!(
            request
                .context
                .get("tokenOut")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("ETH")
        );
        assert_eq!(
            request.context.get("recipient").and_then(Value::as_str),
            Some(from.to_string().as_str())
        );
        assert!(request.context.get("amountOut").is_none());
        // `ticket` is intentionally omitted — not declared in the schema.
        assert!(request.context.get("ticket").is_none());
    }

    #[test]
    fn claim_restake_withdrawal_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut action = claim_restake_withdrawal(from.clone());
        action.amount_out = Some(amount(AmountKind::Exact, "999000000000000000"));

        let request = policy_request(&envelope(Action::ClaimRestakeWithdrawal(action)), &from);

        assert_eq!(
            request
                .context
                .get("amountOut")
                .and_then(|amount| amount.get("value"))
                .and_then(Value::as_str),
            Some("999000000000000000")
        );
        // `ticket` is required on the struct but must stay out of the context.
        assert!(request.context.get("ticket").is_none());
    }
}
