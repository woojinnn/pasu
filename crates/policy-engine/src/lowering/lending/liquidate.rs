use crate::action::lending::LiquidateAction;
use crate::context_keys::{
    BORROWER, COLLATERAL_ASSET, DEBT_ASSET, DEBT_TO_COVER, LIQUIDATE_MODE, LIQUIDATION_KIND,
    MARKET, RECEIVE_A_TOKEN, RECIPIENT, SEIZED_COLLATERAL_AMOUNT,
};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{liquidate_mode_str, liquidation_kind_str, market_json};

const ACTION_ID: &str = "liquidate";

impl Lower for LiquidateAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(market) = &self.market {
            context.insert(MARKET.into(), market_json(market));
        }
        context.insert(BORROWER.into(), Value::from(self.borrower.to_string()));
        if let Some(collateral_asset) = &self.collateral_asset {
            context.insert(COLLATERAL_ASSET.into(), asset_ref_json(collateral_asset));
        }
        context.insert(DEBT_ASSET.into(), asset_ref_json(&self.debt_asset));
        if let Some(debt_to_cover) = &self.debt_to_cover {
            context.insert(DEBT_TO_COVER.into(), amount_constraint_json(debt_to_cover));
        }
        if let Some(seized_collateral_amount) = &self.seized_collateral_amount {
            context.insert(
                SEIZED_COLLATERAL_AMOUNT.into(),
                amount_constraint_json(seized_collateral_amount),
            );
        }
        context.insert(
            LIQUIDATION_KIND.into(),
            Value::from(liquidation_kind_str(&self.liquidation_kind)),
        );
        if let Some(mode) = &self.liquidate_mode {
            context.insert(LIQUIDATE_MODE.into(), Value::from(liquidate_mode_str(mode)));
        }
        if let Some(recipient) = &self.recipient {
            context.insert(RECIPIENT.into(), Value::from(recipient.to_string()));
        }
        if let Some(receive_a_token) = self.receive_a_token {
            context.insert(RECEIVE_A_TOKEN.into(), Value::from(receive_a_token));
        }

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::lending::{LiquidateAction, LiquidateMode, LiquidationKind};
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, amount, envelope, erc20, market, policy_request,
    };

    fn liquidate(borrower: crate::action::Address) -> LiquidateAction {
        LiquidateAction {
            market: None,
            borrower,
            collateral_asset: None,
            debt_asset: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            debt_to_cover: None,
            seized_collateral_amount: None,
            liquidation_kind: LiquidationKind::PoolShare,
            liquidate_mode: None,
            recipient: None,
            receive_a_token: None,
        }
    }

    #[test]
    fn liquidate_action_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let borrower = address("0x4444444444444444444444444444444444444444");
        let request = policy_request(
            &envelope(Action::Liquidate(liquidate(borrower.clone()))),
            &from,
        );

        assert!(request.action.contains("liquidate"));
        assert_eq!(
            request.context.get("borrower").and_then(Value::as_str),
            Some(borrower.to_string().as_str())
        );
        assert_eq!(
            request
                .context
                .get("debtAsset")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("USDC")
        );
        assert_eq!(
            request.context.get("liquidationKind").and_then(Value::as_str),
            Some("pool_share")
        );
        assert!(request.context.get("market").is_none());
        assert!(request.context.get("collateralAsset").is_none());
        assert!(request.context.get("debtToCover").is_none());
        assert!(request.context.get("seizedCollateralAmount").is_none());
        assert!(request.context.get("liquidateMode").is_none());
        assert!(request.context.get("recipient").is_none());
        assert!(request.context.get("receiveAToken").is_none());
    }

    #[test]
    fn liquidate_action_lowers_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let borrower = address("0x4444444444444444444444444444444444444444");
        let recipient = address("0x5555555555555555555555555555555555555555");
        let mut action = liquidate(borrower);
        action.market = Some(market());
        action.collateral_asset = Some(erc20(
            "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            "WETH",
            18,
        ));
        action.debt_to_cover = Some(amount(AmountKind::Exact, "1000000000"));
        action.seized_collateral_amount = Some(amount(AmountKind::Estimated, "100000000"));
        action.liquidation_kind = LiquidationKind::Socializable;
        action.liquidate_mode = Some(LiquidateMode::Seize);
        action.recipient = Some(recipient.clone());
        action.receive_a_token = Some(true);

        let request = policy_request(&envelope(Action::Liquidate(action)), &from);

        assert_eq!(
            request.context.get("liquidationKind").and_then(Value::as_str),
            Some("socializable")
        );
        assert_eq!(
            request.context.get("liquidateMode").and_then(Value::as_str),
            Some("seize")
        );
        assert_eq!(
            request.context.get("recipient").and_then(Value::as_str),
            Some(recipient.to_string().as_str())
        );
        assert_eq!(
            request.context.get("receiveAToken").and_then(Value::as_bool),
            Some(true)
        );
        assert!(request.context.get("debtToCover").is_some());
        assert!(request.context.get("seizedCollateralAmount").is_some());
        assert_eq!(
            request
                .context
                .get("collateralAsset")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("WETH")
        );
    }

    /// End-to-end coverage that the `liquidationKind` enum lowering reaches
    /// Cedar with the exact snake_case spelling `liquidation_kind_str`
    /// produces. The schema declares `liquidationKind: String`, so a Cedar
    /// `==` against the emitted literal is the right type-correct gate.
    ///
    /// Two evaluations prove the enum flows through:
    ///   * `LiquidationKind::PoolShare` (lowered as `"pool_share"`) → forbid
    ///     fires → `Verdict::Fail`.
    ///   * `LiquidationKind::Socializable` (lowered as `"socializable"`) →
    ///     no match → `Verdict::Pass`.
    #[test]
    fn liquidate_policy_on_liquidation_kind_evaluates_end_to_end() {
        use crate::policy::{PolicyEngineBuilder, Severity, Verdict};

        const LIQUIDATE_SCHEMA: &str =
            include_str!("../../../../../policy-schema/actions/lending/liquidate.cedarschema");

        let policy = r#"
            @id("user/liquidate-deny-pool-share")
            @severity("deny")
            @reason("Pool-share liquidations disallowed")
            forbid (principal, action == Action::"liquidate", resource)
            when {
              context.liquidationKind == "pool_share"
            };
        "#;

        let engine = PolicyEngineBuilder::new()
            .add_schema_text(LIQUIDATE_SCHEMA)
            .add_text(policy)
            .build()
            .expect("liquidate policy strict-validates against the bundled schema");

        let from = address("0x1111111111111111111111111111111111111111");
        let borrower = address("0x4444444444444444444444444444444444444444");

        // PoolShare → forbid fires.
        let mut pool_share = liquidate(borrower.clone());
        pool_share.liquidation_kind = LiquidationKind::PoolShare;
        let pool_share_request =
            policy_request(&envelope(Action::Liquidate(pool_share)), &from);

        match engine
            .evaluate_request(&pool_share_request)
            .expect("engine evaluates lowered liquidate request (pool_share)")
        {
            Verdict::Fail(matched) => {
                assert_eq!(matched.len(), 1);
                assert_eq!(matched[0].policy_id, "user/liquidate-deny-pool-share");
                assert_eq!(matched[0].severity, Severity::Deny);
            }
            other => panic!(
                "expected Verdict::Fail for PoolShare liquidation, got {other:?}"
            ),
        }

        // Socializable → no match → Pass.
        let mut socializable = liquidate(borrower);
        socializable.liquidation_kind = LiquidationKind::Socializable;
        let socializable_request =
            policy_request(&envelope(Action::Liquidate(socializable)), &from);

        let pass_verdict = engine
            .evaluate_request(&socializable_request)
            .expect("engine evaluates lowered liquidate request (socializable)");
        assert_eq!(pass_verdict, Verdict::Pass);
    }
}
