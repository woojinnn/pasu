use crate::action::lending::BorrowAction;
use crate::context_keys::{
    AMOUNT, AMOUNT_MODE, ASSET, MARKET, ON_BEHALF, RECIPIENT, VALIDITY_DELTA_SEC,
};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::common::validity::{validity_delta_sec, validity_json};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{amount_mode_str, market_json};

const ACTION_ID: &str = "borrow";
const VALIDITY: &str = "validity";

impl Lower for BorrowAction {
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
        context.insert(ON_BEHALF.into(), Value::from(self.on_behalf.to_string()));
        if let Some(validity) = &self.validity {
            context.insert(VALIDITY.into(), validity_json(validity));
            if let Some(delta_sec) = validity_delta_sec(validity, ctx.block_timestamp) {
                context.insert(VALIDITY_DELTA_SEC.into(), Value::from(delta_sec));
            }
        }

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::lending::{AmountMode, BorrowAction};
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, amount, envelope, erc20, market, policy_request, validity, BLOCK_TIMESTAMP,
    };

    fn borrow(recipient: crate::action::Address, on_behalf: crate::action::Address) -> BorrowAction {
        BorrowAction {
            market: None,
            asset: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            amount: amount(AmountKind::Exact, "1000000000"),
            amount_mode: None,
            recipient,
            on_behalf,
            validity: None,
        }
    }

    #[test]
    fn borrow_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::Borrow(borrow(from.clone(), from.clone()))),
            &from,
        );

        assert!(request.action.contains("borrow"));
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
            Some(from.to_string().as_str())
        );
        assert_eq!(
            request.context.get("onBehalf").and_then(Value::as_str),
            Some(from.to_string().as_str())
        );
        assert!(request.context.get("market").is_none());
        assert!(request.context.get("validity").is_none());
        assert!(request.context.get("validityDeltaSec").is_none());
    }

    #[test]
    fn borrow_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let on_behalf = address("0x3333333333333333333333333333333333333333");
        let mut action = borrow(from.clone(), on_behalf.clone());
        action.market = Some(market());
        action.amount_mode = Some(AmountMode::Assets);
        action.validity = Some(validity(BLOCK_TIMESTAMP + 300));

        let request = policy_request(&envelope(Action::Borrow(action)), &from);

        assert_eq!(
            request.context.get("amountMode").and_then(Value::as_str),
            Some("assets")
        );
        assert_eq!(
            request.context.get("onBehalf").and_then(Value::as_str),
            Some(on_behalf.to_string().as_str())
        );
        assert!(request.context.get("market").is_some());
        assert!(request.context.get("validity").is_some());
        assert_eq!(
            request
                .context
                .get("validityDeltaSec")
                .and_then(Value::as_i64),
            Some(300)
        );
    }

    /// End-to-end coverage that a Cedar policy guarding on
    /// `context.amount.value` evaluates against the lowered `BorrowAction`.
    ///
    /// `AmountConstraint.value` is a `String` in the core schema (BigInt-on-the-
    /// wire, since EVM amounts overflow `Long`), so the policy compares against
    /// the literal decimal string the lowering emits rather than using a
    /// numeric `>`. Two evaluations prove both halves of the gate flow through:
    ///   * `1_000_000_000_000` (large) → matches the forbid → `Verdict::Fail`.
    ///   * `100` (small) → no match → `Verdict::Pass`.
    /// Together they lock that `amount.value` reaches Cedar with the exact
    /// string the lowering produced.
    #[test]
    fn borrow_policy_on_amount_value_evaluates_end_to_end() {
        use crate::policy::{PolicyEngineBuilder, Severity, Verdict};

        const BORROW_SCHEMA: &str =
            include_str!("../../../../../policy-schema/actions/lending/borrow.cedarschema");

        let policy = r#"
            @id("user/borrow-amount-too-large")
            @severity("deny")
            @reason("Borrow amount exceeds limit")
            forbid (principal, action == Action::"borrow", resource)
            when {
              context has amount &&
              context.amount has value &&
              context.amount.value == "1000000000000"
            };
        "#;

        let engine = PolicyEngineBuilder::new()
            .add_schema_text(BORROW_SCHEMA)
            .add_text(policy)
            .build()
            .expect("borrow policy strict-validates against the bundled schema");

        // Large borrow: amount.value == "1000000000000" → forbid fires.
        let from = address("0x1111111111111111111111111111111111111111");
        let on_behalf = address("0x3333333333333333333333333333333333333333");
        let mut large = borrow(from.clone(), on_behalf.clone());
        large.amount = amount(AmountKind::Exact, "1000000000000");
        let large_request =
            policy_request(&envelope(Action::Borrow(large)), &from);

        match engine
            .evaluate_request(&large_request)
            .expect("engine evaluates lowered borrow request (large)")
        {
            Verdict::Fail(matched) => {
                assert_eq!(matched.len(), 1);
                assert_eq!(matched[0].policy_id, "user/borrow-amount-too-large");
                assert_eq!(matched[0].severity, Severity::Deny);
            }
            other => panic!("expected Verdict::Fail for large borrow, got {other:?}"),
        }

        // Small borrow: amount.value == "100" → no match → Pass.
        let mut small = borrow(from.clone(), on_behalf);
        small.amount = amount(AmountKind::Exact, "100");
        let small_request = policy_request(&envelope(Action::Borrow(small)), &from);

        let small_verdict = engine
            .evaluate_request(&small_request)
            .expect("engine evaluates lowered borrow request (small)");
        assert_eq!(small_verdict, Verdict::Pass);
    }
}
