use crate::action::lending::FlashLoanAction;
use crate::action::{AmountConstraint, AmountKind, AssetRef, AssetRefWithAmountConstraint};
use crate::context_keys::{ASSETS, FEE, FLASH_LOAN_KIND, ON_BEHALF, POOL, RECEIVER};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_with_amount_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::common::{flash_loan_kind_str, market_json};

const ACTION_ID: &str = "flash_loan";

impl Lower for FlashLoanAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        let mut context = Map::new();
        if let Some(pool) = &self.pool {
            context.insert(POOL.into(), market_json(pool));
        }
        context.insert(
            ASSETS.into(),
            Value::Array(zip_assets(&self.assets, &self.amounts)),
        );
        context.insert(RECEIVER.into(), Value::from(self.receiver.to_string()));
        if let Some(on_behalf) = &self.on_behalf {
            context.insert(ON_BEHALF.into(), Value::from(on_behalf.to_string()));
        }
        context.insert(
            FLASH_LOAN_KIND.into(),
            Value::from(flash_loan_kind_str(&self.flash_loan_kind)),
        );
        if let Some(fee) = &self.fee {
            context.insert(FEE.into(), amount_constraint_json(fee));
        }

        ctx.request(ACTION_ID, Value::Object(context))
    }
}

/// Pair the parallel `assets` and `amounts` arrays into the
/// `AssetRefWithAmountConstraint` shape Cedar expects.
///
/// The action struct holds two parallel lists (legacy schema). When lengths
/// differ — which the decoder/mapper shouldn't produce, but is defensive
/// against partial decode — surplus assets receive an `Unknown` amount and
/// surplus amounts are dropped. The mismatch surfaces clearly in lowered JSON
/// rather than producing an empty context that silently lets the action
/// through.
fn zip_assets(assets: &[AssetRef], amounts: &[AmountConstraint]) -> Vec<Value> {
    let len = assets.len().max(amounts.len());
    (0..len)
        .map(|index| {
            let asset = assets.get(index).cloned().unwrap_or(AssetRef {
                kind: crate::action::AssetKind::Unknown,
                address: None,
                token_id: None,
                symbol: None,
                decimals: None,
            });
            let amount = amounts.get(index).cloned().unwrap_or(AmountConstraint {
                kind: AmountKind::Unknown,
                value: None,
            });
            asset_ref_with_amount_json(&AssetRefWithAmountConstraint { asset, amount })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::action::lending::{FlashLoanAction, FlashLoanKind};
    use crate::action::{Action, AmountKind};
    use serde_json::Value;

    use crate::lowering::lending::test_support::{
        address, amount, envelope, erc20, market, policy_request,
    };

    fn flash_loan(receiver: crate::action::Address) -> FlashLoanAction {
        FlashLoanAction {
            pool: None,
            assets: vec![erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6)],
            amounts: vec![amount(AmountKind::Exact, "1000000000")],
            receiver,
            on_behalf: None,
            flash_loan_kind: FlashLoanKind::Simple,
            fee: None,
        }
    }

    #[test]
    fn flash_loan_lowers_minimal_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let receiver = address("0x6666666666666666666666666666666666666666");
        let request = policy_request(
            &envelope(Action::FlashLoan(flash_loan(receiver.clone()))),
            &from,
        );

        assert!(request.action.contains("flash_loan"));
        assert_eq!(
            request.context.get("receiver").and_then(Value::as_str),
            Some(receiver.to_string().as_str())
        );
        assert_eq!(
            request.context.get("flashLoanKind").and_then(Value::as_str),
            Some("simple")
        );

        let assets = request
            .context
            .get("assets")
            .and_then(Value::as_array)
            .expect("assets is an array");
        assert_eq!(assets.len(), 1);
        assert_eq!(
            assets[0]
                .get("asset")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("USDC")
        );
        assert_eq!(
            assets[0]
                .get("amount")
                .and_then(|amount| amount.get("value"))
                .and_then(Value::as_str),
            Some("1000000000")
        );

        assert!(request.context.get("pool").is_none());
        assert!(request.context.get("onBehalf").is_none());
        assert!(request.context.get("fee").is_none());
    }

    #[test]
    fn flash_loan_lowers_multi_asset_full_context() {
        let from = address("0x1111111111111111111111111111111111111111");
        let receiver = address("0x6666666666666666666666666666666666666666");
        let on_behalf = address("0x7777777777777777777777777777777777777777");
        let mut action = flash_loan(receiver);
        action.pool = Some(market());
        action.assets = vec![
            erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
        ];
        action.amounts = vec![
            amount(AmountKind::Exact, "1000000000"),
            amount(AmountKind::Exact, "2"),
        ];
        action.on_behalf = Some(on_behalf.clone());
        action.flash_loan_kind = FlashLoanKind::Multi;
        action.fee = Some(amount(AmountKind::Exact, "5"));

        let request = policy_request(&envelope(Action::FlashLoan(action)), &from);

        assert_eq!(
            request.context.get("flashLoanKind").and_then(Value::as_str),
            Some("multi")
        );
        assert_eq!(
            request.context.get("onBehalf").and_then(Value::as_str),
            Some(on_behalf.to_string().as_str())
        );
        assert!(request.context.get("pool").is_some());
        assert!(request.context.get("fee").is_some());

        let assets = request
            .context
            .get("assets")
            .and_then(Value::as_array)
            .expect("assets is an array");
        assert_eq!(assets.len(), 2);
        assert_eq!(
            assets[1]
                .get("asset")
                .and_then(|asset| asset.get("symbol"))
                .and_then(Value::as_str),
            Some("WETH")
        );
    }

    #[test]
    fn flash_loan_with_mismatched_lengths_pads_with_unknown() {
        let from = address("0x1111111111111111111111111111111111111111");
        let receiver = address("0x6666666666666666666666666666666666666666");
        let mut action = flash_loan(receiver);
        action.assets = vec![
            erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
        ];
        // Only one amount for two assets.

        let request = policy_request(&envelope(Action::FlashLoan(action)), &from);

        let assets = request
            .context
            .get("assets")
            .and_then(Value::as_array)
            .expect("assets is an array");
        assert_eq!(assets.len(), 2);
        assert_eq!(
            assets[1]
                .get("amount")
                .and_then(|amount| amount.get("kind"))
                .and_then(Value::as_str),
            Some("unknown")
        );
    }

    /// End-to-end coverage that an action-level blanket deny (no `when`
    /// clause, no context predicate) fires on the `Action::"flash_loan"` UID
    /// the dispatcher emits. This is the lightest possible smoke test that
    /// the action kind alone is sufficient to match — `flashLoanKind`,
    /// `assets`, and the rest of the context never come into play.
    #[test]
    fn flash_loan_policy_blanket_deny_evaluates_end_to_end() {
        use crate::policy::{PolicyEngineBuilder, Severity, Verdict};

        const FLASH_LOAN_SCHEMA: &str = include_str!(
            "../../../../../policy-schema/actions/lending/flash_loan.cedarschema"
        );

        let policy = r#"
            @id("user/flash-loan-blanket-deny")
            @severity("deny")
            @reason("Flash loans disallowed")
            forbid (principal, action == Action::"flash_loan", resource);
        "#;

        let engine = PolicyEngineBuilder::new()
            .add_schema_text(FLASH_LOAN_SCHEMA)
            .add_text(policy)
            .build()
            .expect("flash_loan blanket-deny policy strict-validates against the bundled schema");

        let from = address("0x1111111111111111111111111111111111111111");
        let receiver = address("0x6666666666666666666666666666666666666666");
        let request =
            policy_request(&envelope(Action::FlashLoan(flash_loan(receiver))), &from);

        match engine
            .evaluate_request(&request)
            .expect("engine evaluates lowered flash_loan request")
        {
            Verdict::Fail(matched) => {
                assert_eq!(matched.len(), 1);
                assert_eq!(matched[0].policy_id, "user/flash-loan-blanket-deny");
                assert_eq!(matched[0].severity, Severity::Deny);
            }
            other => panic!(
                "expected Verdict::Fail for blanket-deny flash_loan, got {other:?}"
            ),
        }
    }
}
