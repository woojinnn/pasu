//! Signature `Action` variants to `PolicyRequest` conversion.

use crate::context_keys::{
    AMOUNT_HUMAN, AMOUNT_HUMAN_CLAMPED_AT_CEILING, APPROVAL_COUNT, DEADLINE, DEADLINE_DELTA_SEC,
    DOMAIN_CHAIN_ID, DOMAIN_NAME, DOMAIN_SALT, DOMAIN_VERSION, EXPIRATION, IS_UNLIMITED,
    MESSAGE_JSON, NONCE, NONCE_VALID, NOW_TS, OWNER, PERMIT_KIND, PRIMARY_TYPE, REQUEST_CHAIN_ID,
    SIGNER, SIG_DEADLINE, SIG_DEADLINE_DELTA_SEC, SPENDER, TOKEN, TOTAL_APPROVED_USD, TYPES_JSON,
    VALUE_HUMAN, VALUE_HUMAN_CLAMPED_AT_CEILING, VERIFYING_CONTRACT,
};
use crate::core::{Eip2612Action, Eip712OtherAction, Permit2Action};
use crate::policy::PolicyRequest;
use alloy_primitives::U256;
use serde_json::{json, Map, Value};

use super::amount::{decimal_json, token_json, usd_valuation_json};

const HUMAN_DECIMAL_SCALE: u64 = 10_000;
const CEDAR_DECIMAL_CEILING: &str = "922337203685477.5807";
const HUMAN_INT_CEILING: u128 = 922_337_203_685_477;
const CEDAR_DECIMAL_CEILING_FRACTION: u64 = 5_807;

pub(super) fn permit2_request(action: &Permit2Action, now_ts: u64) -> PolicyRequest {
    PolicyRequest::new(
        principal(action.signer.as_str()),
        r#"Action::"signature.permit2""#,
        r#"Protocol::"signature.permit2""#,
        entities(action.signer.as_str(), "signature.permit2"),
        permit2_context(action, now_ts),
    )
}

pub(super) fn eip2612_request(action: &Eip2612Action, now_ts: u64) -> PolicyRequest {
    PolicyRequest::new(
        principal(action.signer.as_str()),
        r#"Action::"signature.eip2612""#,
        r#"Protocol::"signature.eip2612""#,
        entities(action.signer.as_str(), "signature.eip2612"),
        eip2612_context(action, now_ts),
    )
}

pub(super) fn eip712_other_request(action: &Eip712OtherAction, now_ts: u64) -> PolicyRequest {
    PolicyRequest::new(
        principal(action.signer.as_str()),
        r#"Action::"signature.eip712_other""#,
        r#"Protocol::"signature.eip712_other""#,
        entities(action.signer.as_str(), "signature.eip712_other"),
        eip712_other_context(action, now_ts),
    )
}

fn principal(signer: &str) -> String {
    format!(r#"Wallet::"{signer}""#)
}

fn entities(signer: &str, protocol: &str) -> Value {
    json!([
        { "uid": { "type": "Wallet", "id": signer }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Protocol", "id": protocol }, "attrs": {}, "parents": [] },
    ])
}

fn permit2_context(action: &Permit2Action, now_ts: u64) -> Value {
    let mut context = Map::new();
    common_signature_context(
        &mut context,
        action.signer.as_str(),
        action.chain_id,
        action.domain_chain_id,
        action.verifying_contract.as_str(),
        &action.primary_type,
        now_ts,
    );
    context.insert(PERMIT_KIND.into(), Value::from(action.permit_kind.as_str()));
    context.insert(SPENDER.into(), Value::from(action.spender.as_str()));
    context.insert(TOKEN.into(), token_json(&action.token));
    let (amount_human, amount_clamped) =
        token_amount_human_decimal(&action.amount, action.token.decimals);
    context.insert(AMOUNT_HUMAN.into(), decimal_json(&amount_human));
    context.insert(
        EXPIRATION.into(),
        Value::from(cedar_long_u64(action.expiration)),
    );
    context.insert(
        SIG_DEADLINE.into(),
        Value::from(cedar_long_u64(action.sig_deadline)),
    );
    context.insert(
        SIG_DEADLINE_DELTA_SEC.into(),
        Value::from(deadline_delta(now_ts, action.sig_deadline)),
    );
    context.insert(NONCE.into(), Value::from(action.nonce.as_str()));
    context.insert(
        APPROVAL_COUNT.into(),
        Value::from(cedar_long_usize(action.approvals.len())),
    );
    context.insert(NONCE_VALID.into(), Value::from(action.nonce_valid));
    context.insert(IS_UNLIMITED.into(), Value::from(action.is_unlimited));
    context.insert(
        AMOUNT_HUMAN_CLAMPED_AT_CEILING.into(),
        Value::from(amount_clamped),
    );
    if let Some(usd) = &action.total_approved_usd {
        context.insert(TOTAL_APPROVED_USD.into(), usd_valuation_json(usd));
    }
    Value::Object(context)
}

fn eip2612_context(action: &Eip2612Action, now_ts: u64) -> Value {
    let mut context = Map::new();
    common_signature_context(
        &mut context,
        action.signer.as_str(),
        action.chain_id,
        action.domain_chain_id,
        action.verifying_contract.as_str(),
        &action.primary_type,
        now_ts,
    );
    context.insert(OWNER.into(), Value::from(action.owner.as_str()));
    context.insert(SPENDER.into(), Value::from(action.spender.as_str()));
    context.insert(TOKEN.into(), token_json(&action.token));
    let (value_human, value_clamped) =
        token_amount_human_decimal(&action.value, action.token.decimals);
    context.insert(VALUE_HUMAN.into(), decimal_json(&value_human));
    context.insert(
        DEADLINE.into(),
        Value::from(cedar_long_u64(action.deadline)),
    );
    context.insert(
        DEADLINE_DELTA_SEC.into(),
        Value::from(deadline_delta(now_ts, action.deadline)),
    );
    context.insert(NONCE.into(), Value::from(action.nonce.as_str()));
    context.insert(NONCE_VALID.into(), Value::from(action.nonce_valid));
    context.insert(IS_UNLIMITED.into(), Value::from(action.is_unlimited));
    context.insert(
        VALUE_HUMAN_CLAMPED_AT_CEILING.into(),
        Value::from(value_clamped),
    );
    if let Some(usd) = &action.total_approved_usd {
        context.insert(TOTAL_APPROVED_USD.into(), usd_valuation_json(usd));
    }
    Value::Object(context)
}

fn eip712_other_context(action: &Eip712OtherAction, now_ts: u64) -> Value {
    let mut context = Map::new();
    common_signature_context(
        &mut context,
        action.signer.as_str(),
        action.chain_id,
        action.domain_chain_id,
        action.verifying_contract.as_str(),
        &action.primary_type,
        now_ts,
    );
    context.insert(DOMAIN_NAME.into(), Value::from(action.domain_name.as_str()));
    context.insert(
        DOMAIN_VERSION.into(),
        Value::from(action.domain_version.as_str()),
    );
    context.insert(DOMAIN_SALT.into(), Value::from(action.domain_salt.as_str()));
    context.insert(TYPES_JSON.into(), Value::from(action.types_json.as_str()));
    context.insert(
        MESSAGE_JSON.into(),
        Value::from(action.message_json.as_str()),
    );
    Value::Object(context)
}

fn common_signature_context(
    context: &mut Map<String, Value>,
    signer: &str,
    request_chain_id: u64,
    domain_chain_id: u64,
    verifying_contract: &str,
    primary_type: &str,
    now_ts: u64,
) {
    context.insert(SIGNER.into(), Value::from(signer));
    context.insert(
        REQUEST_CHAIN_ID.into(),
        Value::from(cedar_long_u64(request_chain_id)),
    );
    context.insert(
        DOMAIN_CHAIN_ID.into(),
        Value::from(cedar_long_u64(domain_chain_id)),
    );
    context.insert(VERIFYING_CONTRACT.into(), Value::from(verifying_contract));
    context.insert(PRIMARY_TYPE.into(), Value::from(primary_type));
    context.insert(NOW_TS.into(), Value::from(cedar_long_u64(now_ts)));
}

fn deadline_delta(now_ts: u64, deadline: u64) -> i64 {
    cedar_long_u64(deadline.saturating_sub(now_ts))
}

fn token_amount_human_decimal(raw: &str, decimals: u32) -> (String, bool) {
    let raw = U256::from_str_radix(raw, 10).unwrap_or(U256::ZERO);
    let token_scale = U256::from(10u64).pow(U256::from(decimals));
    let integer_part = raw / token_scale;

    if integer_part > U256::from(HUMAN_INT_CEILING) {
        return (CEDAR_DECIMAL_CEILING.into(), true);
    }

    let fractional_raw = raw % token_scale;
    let fractional_part =
        fractional_raw.saturating_mul(U256::from(HUMAN_DECIMAL_SCALE)) / token_scale;

    if exceeds_cedar_decimal_ceiling(integer_part, fractional_part) {
        return (CEDAR_DECIMAL_CEILING.into(), true);
    }

    (
        format!("{integer_part}.{}", four_digit_fraction(fractional_part)),
        false,
    )
}

fn exceeds_cedar_decimal_ceiling(integer_part: U256, fractional_part: U256) -> bool {
    let ceiling_integer = U256::from(HUMAN_INT_CEILING);
    integer_part > ceiling_integer
        || (integer_part == ceiling_integer
            && fractional_part > U256::from(CEDAR_DECIMAL_CEILING_FRACTION))
}

fn four_digit_fraction(value: U256) -> String {
    let value = value.to_string();
    if value.len() >= 4 {
        value
    } else {
        format!("{}{}", "0".repeat(4 - value.len()), value)
    }
}

fn cedar_long_usize(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn cedar_long_u64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UINT160_MAX: &str = "1461501637330902918203684832716283019655932542975";
    const UINT256_MAX: &str =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";

    #[test]
    fn token_amount_human_decimal_formats_regular_amount() {
        assert_eq!(
            token_amount_human_decimal("10000000", 6),
            ("10.0000".into(), false)
        );
    }

    #[test]
    fn token_amount_human_decimal_allows_ceiling_integer_part() {
        let raw = U256::from(HUMAN_INT_CEILING) * U256::from(1_000_000u64);

        assert_eq!(
            token_amount_human_decimal(&raw.to_string(), 6),
            ("922337203685477.0000".into(), false)
        );
    }

    #[test]
    fn token_amount_human_decimal_clamps_above_ceiling_integer_part() {
        let raw = U256::from(HUMAN_INT_CEILING + 1) * U256::from(1_000_000u64);

        assert_eq!(
            token_amount_human_decimal(&raw.to_string(), 6),
            ("922337203685477.5807".into(), true)
        );
    }

    #[test]
    fn token_amount_human_decimal_clamps_uint160_max_without_panic() {
        assert_eq!(
            token_amount_human_decimal(UINT160_MAX, 6),
            ("922337203685477.5807".into(), true)
        );
    }

    #[test]
    fn token_amount_human_decimal_clamps_uint256_max_without_panic() {
        assert_eq!(
            token_amount_human_decimal(UINT256_MAX, 18),
            ("922337203685477.5807".into(), true)
        );
    }
}
