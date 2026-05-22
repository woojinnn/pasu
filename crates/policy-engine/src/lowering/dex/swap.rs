use crate::action::dex::{SwapAction, SwapMode};
use crate::action::AssetRefWithAmountConstraint;
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
const FEE_BPS_KEY: &str = FEE_BPS;
const INPUT_AMOUNT_NANO: &str = "inputAmountNano";
const OUTPUT_AMOUNT_NANO: &str = "outputAmountNano";

/// Decimal-point exponent every token-native amount field shares. Raw on-chain
/// `amount.value` is rescaled by `10^(NANO_SCALE − decimals)` so all tokens
/// land in the same Gwei-style unit (`1 token = 10^9`) regardless of their
/// native `decimals`. Matches the policy-builder side's `scale = 9` so a
/// user typing `> 0.5` ends up comparing against `> 500000000` here.
const NANO_SCALE: u32 = 9;

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
        context.insert(FEE_BPS_KEY.into(), cedar_long_u64(u64::from(fee_bps)));
    }

    // Engine-computed nano normalization: any token-native amount policy
    // that used to depend on the manifest's `token.normalize_to_nano`
    // requirement now reads this base field instead. We compute
    // best-effort — when `amount.value` is absent (e.g. an `unlimited`
    // constraint), or `asset.decimals` is missing, or the rescaled value
    // exceeds `i64::MAX`, the field stays absent and the policy's
    // `has` guard fail-opens it. Better to leave it undefined than to
    // emit a deceptively-clamped Long.
    if let Some(nano) = nano_amount(&swap.input_token) {
        context.insert(INPUT_AMOUNT_NANO.into(), Value::from(nano));
    }
    if let Some(nano) = nano_amount(&swap.output_token) {
        context.insert(OUTPUT_AMOUNT_NANO.into(), Value::from(nano));
    }
    // Post-Phase-2: `validityDeltaSec` is manifest-driven enrichment produced
    // by `clock.validity_delta_sec`, no longer derived from `block_timestamp`
    // host-side.
    Ok(Value::Object(context))
}

/// Compute the nano-scaled amount for one side of the swap.
///
/// Returns `None` when the field can't be populated reliably (missing
/// amount value, missing decimals, or overflow during the rescale). The
/// engine then omits the key so the policy's `has` guard reports its
/// absence — same fail-open semantics every other optional field
/// already uses.
fn nano_amount(token: &AssetRefWithAmountConstraint) -> Option<i64> {
    let amount_str = token.amount.value.as_ref()?;
    let decimals = token.asset.decimals?;
    let wei: u128 = amount_str.to_string().parse().ok()?;
    let scale: u32 = u32::from(decimals);
    let nano: u128 = if scale >= NANO_SCALE {
        wei.checked_div(10u128.checked_pow(scale - NANO_SCALE)?)?
    } else {
        wei.checked_mul(10u128.checked_pow(NANO_SCALE - scale)?)?
    };
    i64::try_from(nano).ok()
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
    use crate::action::misc::DelegateAction;
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
        // `Delegate` has no lowering arm (it hits the dispatcher's `_ =>
        // Ok(None)` catch-all), so the dispatcher must return `None`.
        // Phase 7B note: `Approve` / `SetApprovalForAll` used to fill this
        // role but are now lowered, so this regression uses `Delegate`.
        let envelope = crate::action::ActionEnvelope {
            category: Category::Misc,
            action: Action::Delegate(DelegateAction {
                token: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "GOV", 18),
                delegatee: address("0x2222222222222222222222222222222222222222"),
                delegatee_label: None,
                current_delegate: None,
                voting_power: None,
                power_type: None,
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
