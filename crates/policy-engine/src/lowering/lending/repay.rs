use crate::action::lending::RepayAction;
use crate::context_keys::{
    AMOUNT, AMOUNT_MODE, ASSET, MARKET, ON_BEHALF, REPAY_KIND, VALIDITY_DELTA_SEC,
};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::common::validity::{validity_delta_sec, validity_json};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{amount_mode_str, market_json, repay_kind_str};

const ACTION_ID: &str = "repay";
const VALIDITY: &str = "validity";

impl Lower for RepayAction {
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
        context.insert(ON_BEHALF.into(), Value::from(self.on_behalf.to_string()));
        context.insert(
            REPAY_KIND.into(),
            Value::from(repay_kind_str(&self.repay_kind)),
        );
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
    use crate::action::lending::{AmountMode, RepayAction, RepayKind};
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, amount, envelope, erc20, market, policy_request, validity, BLOCK_TIMESTAMP,
    };

    fn repay(on_behalf: crate::action::Address) -> RepayAction {
        RepayAction {
            market: None,
            asset: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            amount: amount(AmountKind::Exact, "1000000000"),
            amount_mode: None,
            on_behalf,
            repay_kind: RepayKind::DebtAsset,
            validity: None,
        }
    }

    #[test]
    fn repay_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(&envelope(Action::Repay(repay(from.clone()))), &from);

        assert!(request.action.contains("repay"));
        assert_eq!(
            request.context.get("onBehalf").and_then(Value::as_str),
            Some(from.to_string().as_str())
        );
        assert_eq!(
            request.context.get("repayKind").and_then(Value::as_str),
            Some("debt_asset")
        );
        assert!(request.context.get("market").is_none());
        assert!(request.context.get("amountMode").is_none());
        assert!(request.context.get("validity").is_none());
    }

    #[test]
    fn repay_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let position_owner = address("0x3333333333333333333333333333333333333333");
        let mut action = repay(position_owner.clone());
        action.market = Some(market());
        action.amount_mode = Some(AmountMode::Shares);
        action.repay_kind = RepayKind::AtokenDirect;
        action.validity = Some(validity(BLOCK_TIMESTAMP + 60));

        let request = policy_request(&envelope(Action::Repay(action)), &from);

        assert_eq!(
            request.context.get("amountMode").and_then(Value::as_str),
            Some("shares")
        );
        assert_eq!(
            request.context.get("repayKind").and_then(Value::as_str),
            Some("atoken_direct")
        );
        assert_eq!(
            request.context.get("onBehalf").and_then(Value::as_str),
            Some(position_owner.to_string().as_str())
        );
        assert!(request.context.get("market").is_some());
        assert!(request.context.get("validity").is_some());
        assert_eq!(
            request
                .context
                .get("validityDeltaSec")
                .and_then(Value::as_i64),
            Some(60)
        );
    }

    /// End-to-end coverage that the `repayKind` enum lowering reaches Cedar
    /// untouched. `RepayAction.repay_kind` is required (non-optional) and the
    /// lowering serializes it via `repay_kind_str` to the snake_case strings
    /// `"debt_asset"` / `"atoken_direct"` — so the only way to verify the
    /// enum flows correctly is to install a policy that gates on the exact
    /// emitted spelling and assert the verdict flips with the variant.
    ///
    /// Two evaluations prove both branches reach Cedar:
    ///   * `RepayKind::AtokenDirect` (lowered as `"atoken_direct"`) →
    ///     `Verdict::Fail` on the forbid clause.
    ///   * `RepayKind::DebtAsset` (lowered as `"debt_asset"`) → no match →
    ///     `Verdict::Pass`.
    #[test]
    fn repay_policy_on_repay_kind_evaluates_end_to_end() {
        use crate::policy::{PolicyEngineBuilder, Severity, Verdict};

        const REPAY_SCHEMA: &str =
            include_str!("../../../../../policy-schema/actions/lending/repay.cedarschema");

        let policy = r#"
            @id("user/repay-deny-atoken-direct")
            @severity("deny")
            @reason("Direct aToken repay disallowed")
            forbid (principal, action == Action::"repay", resource)
            when {
              context.repayKind == "atoken_direct"
            };
        "#;

        let engine = PolicyEngineBuilder::new()
            .add_schema_text(REPAY_SCHEMA)
            .add_text(policy)
            .build()
            .expect("repay policy strict-validates against the bundled schema");

        let from = address("0x1111111111111111111111111111111111111111");

        // AtokenDirect → forbid fires.
        let mut atoken = repay(from.clone());
        atoken.repay_kind = RepayKind::AtokenDirect;
        let atoken_request =
            policy_request(&envelope(Action::Repay(atoken)), &from);

        match engine
            .evaluate_request(&atoken_request)
            .expect("engine evaluates lowered repay request (atoken_direct)")
        {
            Verdict::Fail(matched) => {
                assert_eq!(matched.len(), 1);
                assert_eq!(matched[0].policy_id, "user/repay-deny-atoken-direct");
                assert_eq!(matched[0].severity, Severity::Deny);
            }
            other => panic!(
                "expected Verdict::Fail for AtokenDirect repay, got {other:?}"
            ),
        }

        // DebtAsset → no match → Pass.
        let mut debt = repay(from.clone());
        debt.repay_kind = RepayKind::DebtAsset;
        let debt_request = policy_request(&envelope(Action::Repay(debt)), &from);

        let debt_verdict = engine
            .evaluate_request(&debt_request)
            .expect("engine evaluates lowered repay request (debt_asset)");
        assert_eq!(debt_verdict, Verdict::Pass);
    }
}
