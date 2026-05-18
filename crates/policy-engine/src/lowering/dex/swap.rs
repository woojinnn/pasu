use crate::action::dex::{SwapAction, SwapMode};
use crate::context_keys::{FEE_BPS, RECIPIENT};
use crate::lowering::dex::asset_with_amount_json;
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

use crate::lowering::common::cedar::cedar_long_u64;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};

const ACTION_ID: &str = "swap";

const SWAP_MODE: &str = "swapMode";
const INPUT_TOKEN: &str = "inputToken";
const OUTPUT_TOKEN: &str = "outputToken";
const VALIDITY: &str = "validity";

impl Lower for SwapAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(swap: &SwapAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(
        SWAP_MODE.into(),
        Value::from(swap_mode_str(&swap.swap_mode)),
    );
    context.insert(
        INPUT_TOKEN.into(),
        asset_with_amount_json(&swap.input_token)?,
    );
    context.insert(
        OUTPUT_TOKEN.into(),
        asset_with_amount_json(&swap.output_token)?,
    );
    context.insert(RECIPIENT.into(), Value::from(swap.recipient.to_string()));

    if let Some(validity) = &swap.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
    }
    if let Some(fee_bps) = swap.fee_bps {
        context.insert(FEE_BPS.into(), cedar_long_u64(u64::from(fee_bps)));
    }
    // Post-Phase-2: `validityDeltaSec` is manifest-driven enrichment produced
    // by `clock.validity_delta_sec`, no longer derived from `block_timestamp`
    // host-side.
    Ok(Value::Object(context))
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
    use crate::action::dex::{SwapAction, SwapMode};
    use crate::action::misc::{ApprovalKind, ApproveAction};
    use crate::action::{
        Action, AmountConstraint, AmountKind, AssetRefWithAmountConstraint, Category,
    };
    use serde_json::{json, Value};

    use crate::lowering::dex::test_support::{
        address, amount, amount_without_value, decimal, envelope, erc20, policy_request, validity,
        BLOCK_TIMESTAMP,
    };

    fn swap(recipient: crate::action::Address, amount_in: AmountConstraint) -> SwapAction {
        let input_asset = erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18);
        let output_asset = erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6);
        SwapAction {
            swap_mode: SwapMode::ExactIn,
            input_token: AssetRefWithAmountConstraint {
                asset: input_asset,
                amount: amount_in,
            },
            output_token: AssetRefWithAmountConstraint {
                asset: output_asset,
                amount: amount(AmountKind::Min, "0"),
            },
            recipient,
            validity: None,
            fee_bps: Some(30),
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
            request
                .context
                .get("inputToken")
                .and_then(|token| token.get("asset")),
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
                .get("outputToken")
                .and_then(|token| token.get("asset"))
                .and_then(|asset| asset.get("address"))
                .and_then(Value::as_str),
            Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        );
        assert!(request.context.get("totalInputUsd").is_none());
        assert!(request.context.get("totalMinOutputUsd").is_none());
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

        assert!(crate::lowering::try_policy_request_from_envelope(
            &envelope,
            &address("0x1111111111111111111111111111111111111111"),
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            BLOCK_TIMESTAMP,
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn swap_with_validity_passes_validity_through_to_context() {
        // Post-Phase-2 the lowering no longer derives `validityDeltaSec`
        // host-side — the matching policy manifest produces it via an RPC
        // call. The lowering simply forwards the validity object verbatim.
        let from = address("0x1111111111111111111111111111111111111111");
        let mut swap = swap(
            from.clone(),
            amount(AmountKind::Exact, "1000000000000000000"),
        );
        swap.validity = Some(validity(BLOCK_TIMESTAMP + 600));

        let request = policy_request(&envelope(Action::Swap(swap)), &from);

        assert!(request
            .context
            .get("validity")
            .and_then(Value::as_object)
            .is_some());
        assert!(!request
            .context
            .as_object()
            .expect("context is an object")
            .contains_key("validityDeltaSec"));
    }

    #[test]
    fn swap_without_validity_omits_validity_block() {
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
            .contains_key("validity"));
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
            .get("inputToken")
            .and_then(|token| token.get("amount"))
            .and_then(Value::as_object)
            .expect("inputToken.amount is an object");

        assert_eq!(
            amount_in.get("kind").and_then(Value::as_str),
            Some("unlimited")
        );
        assert!(!amount_in.contains_key("value"));
    }
}
