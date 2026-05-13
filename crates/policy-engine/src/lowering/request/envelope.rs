//! `ActionEnvelope` to `PolicyRequest` conversion for swap actions.

use crate::action::dex::{SwapAction, SwapMode};
use crate::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
    DecimalString, UsdValuation as ActionUsdValuation, Validity, ValiditySource,
};
use crate::core::UsdValuation;
use crate::policy::PolicyRequest;
use serde_json::{json, Map, Value};

use super::amount::usd_valuation_json;

const ACTION_ID: &str = "swap";

const SWAP_MODE: &str = "swapMode";
const TOKEN_IN: &str = "tokenIn";
const TOKEN_OUT: &str = "tokenOut";
const AMOUNT_IN: &str = "amountIn";
const AMOUNT_OUT: &str = "amountOut";
const RECIPIENT: &str = "recipient";
const VALIDITY: &str = "validity";
const FEE_BPS: &str = "feeBps";
const TOTAL_INPUT_USD: &str = "totalInputUsd";
const TOTAL_MIN_OUTPUT_USD: &str = "totalMinOutputUsd";
const ALLOWANCES_COVER_INPUTS: &str = "allowancesCoverInputs";
const VALIDITY_DELTA_SEC: &str = "validityDeltaSec";

const CHAIN_ID: &str = "chainId";
const ADDRESS: &str = "address";
const SYMBOL: &str = "symbol";
const DECIMALS: &str = "decimals";
const IS_NATIVE: &str = "isNative";
const KIND: &str = "kind";
const VALUE: &str = "value";
const EXPIRES_AT: &str = "expiresAt";
const SOURCE: &str = "source";

/// Build a swap policy request from a normalized swap action envelope.
#[must_use]
pub fn policy_request_from_envelope(
    envelope: &ActionEnvelope,
    from: &Address,
    _to: &Address,
    _value_wei: &DecimalString,
    _chain_id: u64,
    block_timestamp: u64,
) -> Option<PolicyRequest> {
    let Action::Swap(swap) = &envelope.action else {
        return None;
    };

    let wallet_id = from.to_string();
    let principal = format!(r#"Wallet::"{from}""#);
    let resource = r#"Protocol::"swap""#.to_string();
    let entities = json!([
        {
            "uid": { "type": "Wallet", "id": wallet_id.as_str() },
            "attrs": { "address": wallet_id.as_str() },
            "parents": []
        },
        { "uid": { "type": "Protocol", "id": ACTION_ID }, "attrs": {}, "parents": [] },
    ]);

    Some(PolicyRequest::new(
        principal,
        r#"Action::"swap""#,
        resource,
        entities,
        context(swap, block_timestamp),
    ))
}

fn context(swap: &SwapAction, block_timestamp: u64) -> Value {
    let mut context = Map::new();
    context.insert(SWAP_MODE.into(), Value::from(swap_mode_str(&swap.mode)));
    context.insert(TOKEN_IN.into(), asset_ref_json(&swap.token_in));
    context.insert(TOKEN_OUT.into(), asset_ref_json(&swap.token_out));
    context.insert(AMOUNT_IN.into(), amount_constraint_json(&swap.amount_in));
    context.insert(AMOUNT_OUT.into(), amount_constraint_json(&swap.amount_out));
    context.insert(RECIPIENT.into(), Value::from(swap.recipient.to_string()));

    if let Some(validity) = &swap.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
        if let Some(delta_sec) = validity_delta_sec(validity, block_timestamp) {
            context.insert(VALIDITY_DELTA_SEC.into(), Value::from(delta_sec));
        }
    }
    if let Some(fee_bps) = swap.fee_bps {
        context.insert(FEE_BPS.into(), cedar_long_u64(u64::from(fee_bps)));
    }
    if let Some(usd) = &swap.enrichment.min_value_out_usd {
        context.insert(
            TOTAL_MIN_OUTPUT_USD.into(),
            action_usd_valuation_json(usd, block_timestamp),
        );
    }
    if let Some(usd) = &swap.enrichment.value_in_usd {
        context.insert(
            TOTAL_INPUT_USD.into(),
            action_usd_valuation_json(usd, block_timestamp),
        );
    }
    if let Some(covers_input) = swap.enrichment.allowance_covers_input {
        context.insert(ALLOWANCES_COVER_INPUTS.into(), Value::from(covers_input));
    }

    Value::Object(context)
}

fn asset_ref_json(asset: &AssetRef) -> Value {
    let mut out = Map::new();
    out.insert(CHAIN_ID.into(), cedar_long_u64(asset.chain_id));
    out.insert(
        ADDRESS.into(),
        Value::from(
            asset
                .address
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
    );
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

fn amount_constraint_json(amount: &AmountConstraint) -> Value {
    let mut out = Map::new();
    out.insert(KIND.into(), Value::from(amount_kind_str(&amount.kind)));
    if let Some(value) = &amount.value {
        out.insert(VALUE.into(), Value::from(value.to_string()));
    }
    Value::Object(out)
}

const fn swap_mode_str(mode: &SwapMode) -> &'static str {
    match mode {
        SwapMode::ExactIn => "exact_in",
        SwapMode::ExactOut => "exact_out",
        SwapMode::Market => "market",
        SwapMode::Unknown => "unknown",
    }
}

const fn amount_kind_str(kind: &AmountKind) -> &'static str {
    match kind {
        AmountKind::Exact => "exact",
        AmountKind::Min => "min",
        AmountKind::Max => "max",
        AmountKind::Unlimited => "unlimited",
        AmountKind::Estimated => "estimated",
        AmountKind::Unknown => "unknown",
    }
}

fn validity_json(validity: &Validity) -> Value {
    let mut out = Map::new();
    out.insert(
        EXPIRES_AT.into(),
        Value::from(validity.expires_at.to_string()),
    );
    out.insert(
        SOURCE.into(),
        Value::from(validity_source_str(&validity.source)),
    );
    Value::Object(out)
}

fn validity_delta_sec(validity: &Validity, block_timestamp: u64) -> Option<i64> {
    let expires_at = validity.expires_at.to_string().parse::<i64>().ok()?;
    if expires_at < 0 {
        return None;
    }
    let block_timestamp = i64::try_from(block_timestamp).ok()?;
    Some(expires_at - block_timestamp)
}

const fn validity_source_str(source: &ValiditySource) -> &'static str {
    match source {
        ValiditySource::TxDeadline => "tx-deadline",
        ValiditySource::SignatureDeadline => "signature-deadline",
        ValiditySource::GrantExpiration => "grant-expiration",
    }
}

fn action_usd_valuation_json(valuation: &ActionUsdValuation, block_timestamp: u64) -> Value {
    usd_valuation_json(&UsdValuation {
        value: valuation.value.clone(),
        as_of_ts: valuation.as_of_ts.unwrap_or(block_timestamp),
        sources: valuation.sources.clone().unwrap_or_default(),
        stale_sec: valuation.stale_sec.unwrap_or_default(),
    })
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
    use serde_json::{json, Value};
    use std::str::FromStr as _;

    const BLOCK_TIMESTAMP: u64 = 1_700_000_000;

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

    fn swap(recipient: Address, amount_in: AmountConstraint) -> SwapAction {
        SwapAction {
            mode: SwapMode::ExactIn,
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
                expected_value_out_usd: None,
                allowance_covers_input: Some(true),
                input_fraction_of_portfolio_bps: Some(125),
            },
        }
    }

    fn swap_envelope(action: SwapAction) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(action),
        }
    }

    fn validity(expires_at: u64) -> Validity {
        Validity {
            expires_at: decimal(&expires_at.to_string()),
            source: ValiditySource::TxDeadline,
        }
    }

    fn policy_request(envelope: &ActionEnvelope, from: &Address) -> PolicyRequest {
        policy_request_from_envelope(
            envelope,
            from,
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            BLOCK_TIMESTAMP,
        )
        .expect("swap envelope lowers to policy request")
    }

    #[test]
    fn swap_action_lowers_to_policy_request() {
        let from = address("0x1111111111111111111111111111111111111111");
        let request = policy_request(
            &swap_envelope(swap(
                from.clone(),
                amount(AmountKind::Exact, "1000000000000000000"),
            )),
            &from,
        );

        assert_eq!(
            request.principal,
            r#"Wallet::"0x1111111111111111111111111111111111111111""#
        );
        assert!(request.action.contains("swap"));
        assert_eq!(request.resource, r#"Protocol::"swap""#);
        assert_eq!(
            request.entities,
            json!([
                {
                    "uid": { "type": "Wallet", "id": "0x1111111111111111111111111111111111111111" },
                    "attrs": { "address": "0x1111111111111111111111111111111111111111" },
                    "parents": []
                },
                { "uid": { "type": "Protocol", "id": "swap" }, "attrs": {}, "parents": [] },
            ])
        );
        // protocolIds was removed from the swap.cedarschema in commit 9031401.
        assert!(request.context.get("protocolIds").is_none());
        assert_eq!(
            request.context.get("swapMode").and_then(Value::as_str),
            Some("exact_in")
        );
        // tokenIn / tokenOut are now single `Token` objects (Set<Token> removed).
        assert_eq!(
            request
                .context
                .get("tokenIn")
                .and_then(|token| token.get("address"))
                .and_then(Value::as_str),
            Some("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
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
            request.context.get("feeBps").and_then(Value::as_i64),
            Some(30)
        );
    }

    #[test]
    fn non_swap_returns_none() {
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
    fn swap_with_validity_lowers_validity_delta_sec() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut swap = swap(
            from.clone(),
            amount(AmountKind::Exact, "1000000000000000000"),
        );
        swap.validity = Some(validity(BLOCK_TIMESTAMP + 600));

        let request = policy_request(&swap_envelope(swap), &from);

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

        let request = policy_request(&swap_envelope(swap), &from);

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
            &swap_envelope(swap(
                from.clone(),
                amount(AmountKind::Exact, "1000000000000000000"),
            )),
            &from,
        );

        assert!(!request
            .context
            .as_object()
            .unwrap()
            .contains_key("validityDeltaSec"));
    }

    #[test]
    fn swap_with_allowance_covers_input_lowers_field() {
        let from = address("0x1111111111111111111111111111111111111111");
        let mut swap = swap(
            from.clone(),
            amount(AmountKind::Exact, "1000000000000000000"),
        );
        swap.enrichment.allowance_covers_input = Some(true);

        let request = policy_request(&swap_envelope(swap), &from);

        assert_eq!(
            request
                .context
                .get("allowancesCoverInputs")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn external_recipient_does_not_collapse_recipient_field() {
        let from = address("0x1111111111111111111111111111111111111111");
        let recipient = address("0x3333333333333333333333333333333333333333");
        let request = policy_request(
            &swap_envelope(swap(
                recipient.clone(),
                amount(AmountKind::Exact, "1000000000000000000"),
            )),
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
            &swap_envelope(swap(
                from.clone(),
                AmountConstraint {
                    kind: AmountKind::Unlimited,
                    value: None,
                },
            )),
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
