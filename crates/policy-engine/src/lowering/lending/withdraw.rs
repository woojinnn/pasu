use crate::action::lending::WithdrawAction;
use crate::context_keys::{AMOUNT, AMOUNT_MODE, ASSET, MARKET, ON_BEHALF, RECIPIENT};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{amount_mode_str, market_json};

const ACTION_ID: &str = "withdraw";

impl Lower for WithdrawAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(market) = &self.market {
            context.insert(MARKET.into(), market_json(market));
        }
        context.insert(ASSET.into(), asset_ref_json(&self.asset));
        context.insert(AMOUNT.into(), amount_constraint_json(&self.amount));
        if let Some(mode) = &self.amount_mode {
            context.insert(AMOUNT_MODE.into(), Value::from(amount_mode_str(mode)));
        }
        context.insert(RECIPIENT.into(), Value::from(self.recipient.to_string()));
        if let Some(on_behalf) = &self.on_behalf {
            context.insert(ON_BEHALF.into(), Value::from(on_behalf.to_string()));
        }

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::lending::{AmountMode, WithdrawAction};
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, amount, envelope, erc20, market, policy_request,
    };

    fn withdraw(recipient: crate::action::Address) -> WithdrawAction {
        WithdrawAction {
            market: None,
            asset: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            amount: amount(AmountKind::Exact, "1000000000"),
            amount_mode: None,
            recipient,
            on_behalf: None,
        }
    }

    #[test]
    fn withdraw_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(&envelope(Action::Withdraw(withdraw(from.clone()))), &from);

        assert!(request.action.contains("withdraw"));
        assert_eq!(
            request
                .context
                .get("asset")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("USDC")
        );
        assert_eq!(
            request.context.get("recipient").and_then(Value::as_str),
            Some("0x1111111111111111111111111111111111111111")
        );
        assert!(request.context.get("market").is_none());
        assert!(request.context.get("amountMode").is_none());
        assert!(request.context.get("onBehalf").is_none());
    }

    #[test]
    fn withdraw_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let position_owner = address("0x3333333333333333333333333333333333333333");
        let mut action = withdraw(from.clone());
        action.market = Some(market());
        action.amount_mode = Some(AmountMode::Shares);
        action.on_behalf = Some(position_owner.clone());

        let request = policy_request(&envelope(Action::Withdraw(action)), &from);

        assert!(request.context.get("market").is_some());
        assert_eq!(
            request.context.get("amountMode").and_then(Value::as_str),
            Some("shares")
        );
        assert_eq!(
            request.context.get("onBehalf").and_then(Value::as_str),
            Some(position_owner.to_string().as_str())
        );
    }
}
