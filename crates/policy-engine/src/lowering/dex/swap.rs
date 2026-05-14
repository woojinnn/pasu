use crate::action::dex::{SwapAction, SwapMode};
use crate::context_keys::{
    FEE_BPS, RECIPIENT, TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS, TOTAL_INPUT_USD,
    TOTAL_MIN_OUTPUT_USD, VALIDITY_DELTA_SEC,
};
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::amount::{action_usd_valuation_json, amount_constraint_json};
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::common::cedar::cedar_long_u64;
use crate::lowering::common::validity::{validity_delta_sec, validity_json};
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "swap";

const SWAP_MODE: &str = "swapMode";
const TOKEN_IN: &str = "tokenIn";
const TOKEN_OUT: &str = "tokenOut";
const AMOUNT_IN: &str = "amountIn";
const AMOUNT_OUT: &str = "amountOut";
const VALIDITY: &str = "validity";

impl Lower for SwapAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest {
        ctx.request(ACTION_ID, context(self, ctx))
    }
}

fn context(swap: &SwapAction, ctx: &LoweringCtx<'_>) -> Value {
    let mut context = Map::new();
    context.insert(
        SWAP_MODE.into(),
        Value::from(swap_mode_str(&swap.swap_mode)),
    );
    context.insert(TOKEN_IN.into(), asset_ref_json(&swap.token_in));
    context.insert(TOKEN_OUT.into(), asset_ref_json(&swap.token_out));
    context.insert(AMOUNT_IN.into(), amount_constraint_json(&swap.amount_in));
    context.insert(AMOUNT_OUT.into(), amount_constraint_json(&swap.amount_out));
    context.insert(RECIPIENT.into(), Value::from(swap.recipient.to_string()));

    if let Some(validity) = &swap.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
        if let Some(delta_sec) = validity_delta_sec(validity, ctx.block_timestamp) {
            context.insert(VALIDITY_DELTA_SEC.into(), Value::from(delta_sec));
        }
    }
    if let Some(fee_bps) = swap.fee_bps {
        context.insert(FEE_BPS.into(), cedar_long_u64(u64::from(fee_bps)));
    }
    if let Some(usd) = &swap.enrichment.value_in_usd {
        context.insert(
            TOTAL_INPUT_USD.into(),
            action_usd_valuation_json(usd, ctx.block_timestamp),
        );
    }
    if let Some(usd) = &swap.enrichment.min_value_out_usd {
        context.insert(
            TOTAL_MIN_OUTPUT_USD.into(),
            action_usd_valuation_json(usd, ctx.block_timestamp),
        );
    }
    if let Some(fraction_bps) = swap.enrichment.input_fraction_of_portfolio_bps {
        context.insert(
            TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS.into(),
            cedar_long_u64(u64::from(fraction_bps)),
        );
    }

    Value::Object(context)
}

const fn swap_mode_str(mode: &SwapMode) -> &'static str {
    match mode {
        SwapMode::ExactIn => "exact_in",
        SwapMode::ExactOut => "exact_out",
        SwapMode::Market => "market",
        SwapMode::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use crate::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use crate::action::misc::{ApprovalKind, ApproveAction};
    use crate::action::{Action, AmountConstraint, AmountKind, Category};
    use serde_json::{json, Value};

    use crate::lowering::dex::test_support::{
        address, amount, amount_without_value, decimal, envelope, erc20, policy_request, usd,
        validity, BLOCK_TIMESTAMP,
    };

    fn swap(recipient: crate::action::Address, amount_in: AmountConstraint) -> SwapAction {
        SwapAction {
            swap_mode: SwapMode::ExactIn,
            token_in: erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
            token_out: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            amount_in,
            amount_out: amount(AmountKind::Min, "0"),
            recipient,
            validity: None,
            fee_bps: Some(30),
            enrichment: SwapEnrichment {
                value_in_usd: Some(usd("2000.00")),
                min_value_out_usd: Some(usd("0")),
                expected_value_out_usd: Some(usd("2001.00")),
                input_fraction_of_portfolio_bps: Some(125),
            },
        }
    }

    #[test]
    fn swap_action_lowers_to_policy_request() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::Swap(swap(
                from.clone(),
                amount(AmountKind::Exact, "1000000000000000000"),
            ))),
            &from,
        );

        assert_eq!(
            request.principal,
            r#"Wallet::"0x1111111111111111111111111111111111111111""#
        );
        assert!(request.action.contains("swap"));
        // The Protocol resource uid is the transaction target (`to`), so
        // policies can match by router/contract address rather than by action
        // name. policy_request() in test_support uses 0x2222...2 for `to`.
        assert_eq!(
            request.resource,
            r#"Protocol::"0x2222222222222222222222222222222222222222""#
        );
        assert_eq!(
            request.entities,
            json!([
                {
                    "uid": { "type": "Wallet", "id": "0x1111111111111111111111111111111111111111" },
                    "attrs": { "address": "0x1111111111111111111111111111111111111111" },
                    "parents": []
                },
                {
                    "uid": { "type": "Protocol", "id": "0x2222222222222222222222222222222222222222" },
                    "attrs": {},
                    "parents": []
                },
            ])
        );
        assert!(request.context.get("protocolIds").is_none());
        assert_eq!(
            request.context.get("swapMode").and_then(Value::as_str),
            Some("exact_in")
        );
        assert_eq!(
            request.context.get("tokenIn"),
            Some(&json!({
                "kind": "erc20",
                "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                "symbol": "WETH",
                "decimals": 18
            }))
        );
        assert_eq!(
            request
                .context
                .get("tokenOut")
                .and_then(|token| token.get("address"))
                .and_then(Value::as_str),
            Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        );
        assert!(request.context.get("totalInputUsd").is_some());
        assert!(request.context.get("totalMinOutputUsd").is_some());
        assert_eq!(
            request
                .context
                .get("totalInputFractionOfPortfolioBps")
                .and_then(Value::as_i64),
            Some(125)
        );
        assert_eq!(
            request.context.get("feeBps").and_then(Value::as_i64),
            Some(30)
        );
    }

    #[test]
    fn non_dex_action_returns_none() {
        let envelope = crate::action::ActionEnvelope {
            category: Category::Misc,
            action: Action::Approve(ApproveAction {
                token: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
                spender: address("0x2222222222222222222222222222222222222222"),
                spender_label: None,
                amount: amount(AmountKind::Exact, "1000"),
                approval_kind: ApprovalKind::Erc20,
                current_allowance: None,
                validity: None,
            }),
        };

        assert!(crate::lowering::policy_request_from_envelope(
            &envelope,
            &address("0x1111111111111111111111111111111111111111"),
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            BLOCK_TIMESTAMP,
        )
        .is_none());
    }

    #[test]
    fn swap_with_validity_lowers_validity_delta_sec() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut swap = swap(
            from.clone(),
            amount(AmountKind::Exact, "1000000000000000000"),
        );
        swap.validity = Some(validity(BLOCK_TIMESTAMP + 600));

        let request = policy_request(&envelope(Action::Swap(swap)), &from);

        assert_eq!(
            request
                .context
                .get("validityDeltaSec")
                .and_then(Value::as_i64),
            Some(600)
        );
    }

    #[test]
    fn swap_with_past_deadline_lowers_negative_delta() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut swap = swap(
            from.clone(),
            amount(AmountKind::Exact, "1000000000000000000"),
        );
        swap.validity = Some(validity(BLOCK_TIMESTAMP - 60));

        let request = policy_request(&envelope(Action::Swap(swap)), &from);

        assert_eq!(
            request
                .context
                .get("validityDeltaSec")
                .and_then(Value::as_i64),
            Some(-60)
        );
    }

    #[test]
    fn swap_without_validity_omits_validity_delta_sec() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::Swap(swap(
                from.clone(),
                amount(AmountKind::Exact, "1000000000000000000"),
            ))),
            &from,
        );

        assert!(!request
            .context
            .as_object()
            .expect("context is an object")
            .contains_key("validityDeltaSec"));
    }

    #[test]
    fn external_recipient_does_not_collapse_recipient_field() {
        let from = address("0x1111111111111111111111111111111111111111");
        let recipient = address("0x3333333333333333333333333333333333333333");
        let request = policy_request(
            &envelope(Action::Swap(swap(
                recipient.clone(),
                amount(AmountKind::Exact, "1000000000000000000"),
            ))),
            &from,
        );

        assert_eq!(
            request.context.get("recipient").and_then(Value::as_str),
            Some(recipient.to_string().as_str())
        );
    }

    #[test]
    fn unlimited_amount_serializes_without_value_key() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &envelope(Action::Swap(swap(
                from.clone(),
                amount_without_value(AmountKind::Unlimited),
            ))),
            &from,
        );
        let amount_in = request
            .context
            .get("amountIn")
            .and_then(Value::as_object)
            .expect("amountIn is an object");

        assert_eq!(
            amount_in.get("kind").and_then(Value::as_str),
            Some("unlimited")
        );
        assert!(!amount_in.contains_key("value"));
    }
}
