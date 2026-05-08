//! Signature `Action` variants to `PolicyRequest` conversion.

use crate::context_keys::{
    AMOUNT_HUMAN, AMOUNT_HUMAN_CLAMPED_AT_CEILING, APPROVAL_COUNT, BASE, DEADLINE,
    DEADLINE_DELTA_SEC, DOMAIN_CHAIN_ID, DOMAIN_NAME, DOMAIN_SALT, DOMAIN_VERSION, EXPIRATION,
    IS_UNLIMITED, MESSAGE_JSON, NONCE, NONCE_VALID, NOW_TS, OWNER, PERMIT_KIND, PRIMARY_TYPE,
    REQUEST_CHAIN_ID, SIGNER, SIG_DEADLINE, SIG_DEADLINE_DELTA_SEC, SPENDER, TOKEN,
    TOTAL_APPROVED_USD, TYPES_JSON, VALUE_HUMAN, VALUE_HUMAN_CLAMPED_AT_CEILING,
    VERIFYING_CONTRACT, WITNESS_PRESENT,
};
use crate::core::{Eip2612Action, Eip712OtherAction, Permit2Action};
use crate::lowering::decimal::token_amount_human_decimal;
use crate::policy::{PolicyError, PolicyRequest};
use serde_json::{json, Map, Value};

use super::amount::{decimal_json, token_json, usd_valuation_json};

pub(super) fn permit2_request(
    action: &Permit2Action,
    now_ts: u64,
) -> Result<PolicyRequest, PolicyError> {
    Ok(PolicyRequest::new(
        principal(action.signer.as_str()),
        r#"Action::"signature.permit2""#,
        r#"Protocol::"signature.permit2""#,
        entities(action.signer.as_str(), "signature.permit2"),
        permit2_context(action, now_ts)?,
    ))
}

pub(super) fn eip2612_request(
    action: &Eip2612Action,
    now_ts: u64,
) -> Result<PolicyRequest, PolicyError> {
    Ok(PolicyRequest::new(
        principal(action.signer.as_str()),
        r#"Action::"signature.eip2612""#,
        r#"Protocol::"signature.eip2612""#,
        entities(action.signer.as_str(), "signature.eip2612"),
        eip2612_context(action, now_ts)?,
    ))
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

fn permit2_context(action: &Permit2Action, now_ts: u64) -> Result<Value, PolicyError> {
    let mut context = Map::new();
    context.insert(
        BASE.into(),
        signature_base_context(
            action.signer.as_str(),
            action.chain_id,
            action.domain_chain_id,
            action.verifying_contract.as_str(),
            &action.primary_type,
            now_ts,
        ),
    );
    context.insert(PERMIT_KIND.into(), Value::from(action.permit_kind.as_str()));
    context.insert(SPENDER.into(), Value::from(action.spender.as_str()));
    context.insert(TOKEN.into(), token_json(&action.token));
    let (amount_human, amount_clamped) =
        token_amount_human_decimal(&action.amount, action.token.decimals).map_err(|err| {
            PolicyError::Lowering(format!("invalid Permit2 amount {}: {err}", action.amount))
        })?;
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
    context.insert(WITNESS_PRESENT.into(), Value::from(action.witness_present));
    context.insert(
        AMOUNT_HUMAN_CLAMPED_AT_CEILING.into(),
        Value::from(amount_clamped),
    );
    if let Some(usd) = &action.total_approved_usd {
        context.insert(TOTAL_APPROVED_USD.into(), usd_valuation_json(usd));
    }
    Ok(Value::Object(context))
}

fn eip2612_context(action: &Eip2612Action, now_ts: u64) -> Result<Value, PolicyError> {
    let mut context = Map::new();
    context.insert(
        BASE.into(),
        signature_base_context(
            action.signer.as_str(),
            action.chain_id,
            action.domain_chain_id,
            action.verifying_contract.as_str(),
            &action.primary_type,
            now_ts,
        ),
    );
    context.insert(OWNER.into(), Value::from(action.owner.as_str()));
    context.insert(SPENDER.into(), Value::from(action.spender.as_str()));
    context.insert(TOKEN.into(), token_json(&action.token));
    let (value_human, value_clamped) =
        token_amount_human_decimal(&action.value, action.token.decimals).map_err(|err| {
            PolicyError::Lowering(format!("invalid EIP-2612 value {}: {err}", action.value))
        })?;
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
    Ok(Value::Object(context))
}

fn eip712_other_context(action: &Eip712OtherAction, now_ts: u64) -> Value {
    let mut context = Map::new();
    context.insert(
        BASE.into(),
        signature_base_context(
            action.signer.as_str(),
            action.chain_id,
            action.domain_chain_id,
            action.verifying_contract.as_str(),
            &action.primary_type,
            now_ts,
        ),
    );
    if let Some(domain_name) = &action.domain_name {
        context.insert(DOMAIN_NAME.into(), Value::from(domain_name.as_str()));
    }
    if let Some(domain_version) = &action.domain_version {
        context.insert(DOMAIN_VERSION.into(), Value::from(domain_version.as_str()));
    }
    if let Some(domain_salt) = &action.domain_salt {
        context.insert(DOMAIN_SALT.into(), Value::from(domain_salt.as_str()));
    }
    context.insert(TYPES_JSON.into(), Value::from(action.types_json.as_str()));
    context.insert(
        MESSAGE_JSON.into(),
        Value::from(action.message_json.as_str()),
    );
    Value::Object(context)
}

fn signature_base_context(
    signer: &str,
    request_chain_id: u64,
    domain_chain_id: u64,
    verifying_contract: &str,
    primary_type: &str,
    now_ts: u64,
) -> Value {
    let mut context = Map::new();
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
    Value::Object(context)
}

fn deadline_delta(now_ts: u64, deadline: u64) -> i64 {
    cedar_long_u64(deadline.saturating_sub(now_ts))
}

fn cedar_long_usize(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn cedar_long_u64(value: u64) -> i64 {
    let narrowed = i64::try_from(value).unwrap_or(i64::MAX);
    debug_assert!(
        i64::try_from(value).is_ok() || cfg!(test),
        "cedar Long narrowing clamped u64 value {value} to i64::MAX"
    );
    narrowed
}
