//! `ActionEnvelope` to `PolicyRequest` conversion for swap actions.

use crate::action::dex::SwapAction;
use crate::action::{
    Action, ActionEnvelope, Address, AmountKind, AssetKind, AssetRef, DecimalString,
    UsdValuation as ActionUsdValuation,
};
use crate::context_keys::{
    ADDRESS, ALLOWANCES_COVER_INPUTS, CHAIN_ID, DECIMALS, HAS_EXTERNAL_RECIPIENT,
    HAS_ZERO_MIN_OUTPUT, INPUT_TOKENS, IS_NATIVE, MAX_FEE_BPS, OUTPUT_TOKENS, PROTOCOL_IDS, SYMBOL,
    TARGET, TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS, TOTAL_INPUT_USD, TOTAL_MIN_OUTPUT_USD,
    VALUE_WEI,
};
use crate::core::UsdValuation;
use crate::policy::PolicyRequest;
use serde_json::{json, Map, Value};

use super::amount::usd_valuation_json;

const DEFAULT_PROTOCOL_ID: &str = "swap";
const NATIVE_ASSET_ADDRESS: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

/// Build a DEX policy request from a normalized swap action envelope.
#[must_use]
pub fn policy_request_from_envelope(
    envelope: &ActionEnvelope,
    from: &Address,
    to: &Address,
    value_wei: &DecimalString,
    chain_id: u64,
    block_timestamp: u64,
) -> Option<PolicyRequest> {
    let Action::Swap(swap) = &envelope.action else {
        return None;
    };

    let principal = format!(r#"Wallet::"{from}""#);
    let resource = r#"Protocol::"dex""#.to_string();
    let entities = json!([
        { "uid": { "type": "Wallet", "id": from.to_string() }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Protocol", "id": "dex" }, "attrs": {}, "parents": [] },
    ]);

    Some(PolicyRequest::new(
        principal,
        r#"Action::"dex""#,
        resource,
        entities,
        context(swap, from, to, value_wei, chain_id, block_timestamp),
    ))
}

fn context(
    swap: &SwapAction,
    from: &Address,
    to: &Address,
    value_wei: &DecimalString,
    chain_id: u64,
    block_timestamp: u64,
) -> Value {
    let mut context = Map::new();
    context.insert(TARGET.into(), Value::from(to.to_string()));
    context.insert(VALUE_WEI.into(), Value::from(value_wei.to_string()));
    context.insert(PROTOCOL_IDS.into(), json!([DEFAULT_PROTOCOL_ID]));
    context.insert(
        INPUT_TOKENS.into(),
        Value::Array(vec![asset_ref_json(&swap.token_in, chain_id)]),
    );
    context.insert(
        OUTPUT_TOKENS.into(),
        Value::Array(vec![asset_ref_json(&swap.token_out, chain_id)]),
    );
    context.insert(
        HAS_ZERO_MIN_OUTPUT.into(),
        Value::from(has_zero_min_output(swap)),
    );
    context.insert(
        HAS_EXTERNAL_RECIPIENT.into(),
        Value::from(&swap.recipient != from),
    );

    if let Some(usd) = &swap.enrichment.value_in_usd {
        context.insert(
            TOTAL_INPUT_USD.into(),
            action_usd_valuation_json(usd, block_timestamp),
        );
    }
    if let Some(usd) = &swap.enrichment.min_value_out_usd {
        context.insert(
            TOTAL_MIN_OUTPUT_USD.into(),
            action_usd_valuation_json(usd, block_timestamp),
        );
    }
    if let Some(max_fee_bps) = swap.fee_bps {
        context.insert(MAX_FEE_BPS.into(), Value::from(max_fee_bps));
    }
    if let Some(fraction_bps) = swap.enrichment.input_fraction_of_portfolio_bps {
        context.insert(
            TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS.into(),
            cedar_long_u64(u64::from(fraction_bps)),
        );
    }
    if let Some(allowance_covers_input) = swap.enrichment.allowance_covers_input {
        context.insert(
            ALLOWANCES_COVER_INPUTS.into(),
            Value::from(allowance_covers_input),
        );
    }

    Value::Object(context)
}

fn asset_ref_json(asset: &AssetRef, chain_id: u64) -> Value {
    let mut out = Map::new();
    out.insert(CHAIN_ID.into(), cedar_long_u64(chain_id));
    out.insert(ADDRESS.into(), Value::from(asset_address(asset)));
    out.insert(
        SYMBOL.into(),
        Value::from(asset.symbol.as_deref().unwrap_or_default()),
    );
    out.insert(
        DECIMALS.into(),
        Value::from(i64::from(asset.decimals.unwrap_or_default())),
    );
    out.insert(
        IS_NATIVE.into(),
        Value::from(matches!(asset.kind, AssetKind::Native)),
    );
    Value::Object(out)
}

fn asset_address(asset: &AssetRef) -> String {
    asset.address.as_ref().map_or_else(
        || {
            if matches!(asset.kind, AssetKind::Native) {
                NATIVE_ASSET_ADDRESS.to_owned()
            } else {
                String::new()
            }
        },
        ToString::to_string,
    )
}

fn action_usd_valuation_json(valuation: &ActionUsdValuation, block_timestamp: u64) -> Value {
    usd_valuation_json(&UsdValuation {
        value: valuation.value.clone(),
        as_of_ts: valuation.as_of_ts.unwrap_or(block_timestamp),
        sources: valuation.sources.clone().unwrap_or_default(),
        stale_sec: valuation.stale_sec.unwrap_or_default(),
    })
}

fn has_zero_min_output(swap: &SwapAction) -> bool {
    swap.amount_out.kind == AmountKind::Min
        && swap
            .amount_out
            .value
            .as_ref()
            .is_some_and(|value| value.to_string() == "0")
}

fn cedar_long_u64(value: u64) -> Value {
    let narrowed = i64::try_from(value).unwrap_or(i64::MAX);
    debug_assert!(
        i64::try_from(value).is_ok() || cfg!(test),
        "cedar Long narrowing clamped u64 value {value} to i64::MAX"
    );
    Value::from(narrowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use crate::action::misc::{ApprovalKind, ApproveAction};
    use crate::action::{
        Action, AmountConstraint, AmountKind, AssetKind, AssetRef, Category, UsdValuation,
    };
    use crate::context_keys::{
        ADDRESS, ALLOWANCES_COVER_INPUTS, HAS_EXTERNAL_RECIPIENT, HAS_ZERO_MIN_OUTPUT,
        INPUT_TOKENS, MAX_FEE_BPS, OUTPUT_TOKENS, PROTOCOL_IDS,
        TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS, TOTAL_INPUT_USD, TOTAL_MIN_OUTPUT_USD,
    };
    use serde_json::Value;
    use std::str::FromStr as _;

    fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn erc20(address_value: &str, symbol: &str, decimals: u8) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            chain_id: 1,
            address: Some(address(address_value)),
            symbol: Some(symbol.to_owned()),
            decimals: Some(decimals),
        }
    }

    fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    fn usd(value: &str) -> UsdValuation {
        UsdValuation {
            value: value.to_owned(),
            as_of_ts: Some(1_700_000_000),
            sources: Some(vec!["oracle".to_owned()]),
            stale_sec: Some(30),
        }
    }

    fn swap(recipient: Address) -> SwapAction {
        SwapAction {
            mode: SwapMode::ExactIn,
            token_in: erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
            token_out: erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: amount(AmountKind::Min, "0"),
            recipient,
            validity: None,
            fee_bps: Some(30),
            enrichment: SwapEnrichment {
                value_in_usd: Some(usd("2000.00")),
                min_value_out_usd: Some(usd("0")),
                expected_value_out_usd: None,
                allowance_covers_input: Some(true),
                input_fraction_of_portfolio_bps: Some(125),
            },
        }
    }

    fn swap_envelope(recipient: Address) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(swap(recipient)),
        }
    }

    fn policy_request(envelope: &ActionEnvelope, from: &Address) -> PolicyRequest {
        policy_request_from_envelope(
            envelope,
            from,
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            1_700_000_000,
        )
        .expect("swap envelope lowers to policy request")
    }

    #[test]
    fn swap_action_lowers_to_policy_request() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(&swap_envelope(from.clone()), &from);

        assert_eq!(
            request.principal,
            r#"Wallet::"0x1111111111111111111111111111111111111111""#
        );
        assert_eq!(request.action, r#"Action::"dex""#);
        assert_eq!(request.resource, r#"Protocol::"dex""#);
        assert_eq!(
            request
                .context
                .get(PROTOCOL_IDS)
                .and_then(Value::as_array)
                .and_then(|ids| ids.first())
                .and_then(Value::as_str),
            Some("swap")
        );
        assert!(
            request.context.get(TOTAL_INPUT_USD).is_some(),
            "total input USD valuation should be present"
        );
        assert!(
            request.context.get(TOTAL_MIN_OUTPUT_USD).is_some(),
            "minimum output USD valuation should be present"
        );
        assert_eq!(
            request.context.get(MAX_FEE_BPS).and_then(Value::as_u64),
            Some(30)
        );
        assert_eq!(
            request
                .context
                .get(HAS_ZERO_MIN_OUTPUT)
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            request
                .context
                .get(ALLOWANCES_COVER_INPUTS)
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            request
                .context
                .get(TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS)
                .and_then(Value::as_i64),
            Some(125)
        );
        assert_eq!(
            token_address(&request.context, INPUT_TOKENS),
            Some("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
        );
        assert_eq!(
            token_address(&request.context, OUTPUT_TOKENS),
            Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        );
    }

    #[test]
    fn non_swap_action_returns_none() {
        let envelope = ActionEnvelope {
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

        assert!(policy_request_from_envelope(
            &envelope,
            &address("0x1111111111111111111111111111111111111111"),
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            1_700_000_000,
        )
        .is_none());
    }

    #[test]
    fn external_recipient_flag_set() {
        let from = address("0x1111111111111111111111111111111111111111");
        let recipient = address("0x3333333333333333333333333333333333333333");
        let request = policy_request(&swap_envelope(recipient), &from);

        assert_eq!(
            request
                .context
                .get(HAS_EXTERNAL_RECIPIENT)
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    fn token_address<'a>(context: &'a Value, field: &str) -> Option<&'a str> {
        context
            .get(field)
            .and_then(Value::as_array)
            .and_then(|tokens| tokens.first())
            .and_then(|token| token.get(ADDRESS))
            .and_then(Value::as_str)
    }
}
