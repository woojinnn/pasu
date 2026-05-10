//! EIP-712 typed-data 서명 decoder — Permit2 / EIP-2612 / EIP-712 Other / Safe / SessionKey.

use serde_json::Value;

use crate::action::fields::{SignFields, SignSemantic, TokenAmount, TokenAmountWithExpiry};
use crate::semi_adapter::common::{amount_with_unlimited_check, deadline_horizon};
use crate::semi_adapter::error::SemiAdapterError;
use crate::request::TypedDataRequest;
#[cfg(test)]
use crate::request::Eip712Domain;
use crate::types::{Address, AmountSpec, DeadlineFields};

const PERMIT2_CANONICAL: &str = "0x000000000022D473030F116dDEE9F6B43aC78BA3";

/// `TypedDataRequest`에서 `SignFields` 빌드.
///
/// `primary_type`과 `domain.verifyingContract`로 Permit2 / EIP-2612 / Other 분기.
pub fn build_sign_fields(
    req: &TypedDataRequest,
    block_timestamp: Option<u64>,
) -> Result<SignFields, SemiAdapterError> {
    let semantic = decode_semantic(req)?;
    let deadline = extract_deadline(req);

    Ok(SignFields {
        signer: req.signer,
        chain_id: req.chain_id,
        domain: req.domain.clone(),
        primary_type: req.primary_type.clone(),
        semantic,
        deadlines: DeadlineFields {
            deadline,
            deadline_horizon_seconds: deadline.and_then(|d| deadline_horizon(d, block_timestamp)),
        },
    })
}

fn decode_semantic(req: &TypedDataRequest) -> Result<SignSemantic, SemiAdapterError> {
    let is_permit2 = req
        .domain
        .verifying_contract
        .map(|a| {
            format!("{a:#x}").to_lowercase() == PERMIT2_CANONICAL.to_lowercase()
        })
        .unwrap_or(false);

    if is_permit2 {
        return decode_permit2(req);
    }

    if req.primary_type == "Permit" {
        return decode_eip2612(req);
    }

    if req.primary_type == "SafeTx" {
        return decode_safe_tx(req);
    }

    // catch-all
    Ok(SignSemantic::Other {
        types_json: req.types.clone(),
        message_json: req.message.clone(),
    })
}

fn decode_permit2(req: &TypedDataRequest) -> Result<SignSemantic, SemiAdapterError> {
    let m = &req.message;
    match req.primary_type.as_str() {
        "PermitSingle" | "PermitBatch" => {
            let spender = parse_addr(m.get("spender"), "spender")?;
            let nonce = parse_uint_string_field(m, "nonce", "details")?;
            let tokens = if req.primary_type == "PermitSingle" {
                let details = m.get("details").ok_or(SemiAdapterError::MissingArg { name: "details" })?;
                vec![parse_token_amount_with_expiry(details)?]
            } else {
                let details_arr = m.get("details").and_then(|v| v.as_array()).ok_or(SemiAdapterError::MissingArg {
                    name: "details[]",
                })?;
                details_arr
                    .iter()
                    .map(parse_token_amount_with_expiry)
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok(SignSemantic::Permit2Approve { spender, tokens, nonce })
        }
        "PermitTransferFrom" | "PermitBatchTransferFrom" | "PermitWitnessTransferFrom"
        | "PermitBatchWitnessTransferFrom" => {
            let spender = parse_addr(m.get("spender"), "spender")?;
            let nonce = m
                .get("nonce")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or(SemiAdapterError::MissingArg { name: "nonce" })?;
            let permitted = m.get("permitted").ok_or(SemiAdapterError::MissingArg { name: "permitted" })?;
            let transfers = if req.primary_type.contains("Batch") {
                permitted
                    .as_array()
                    .ok_or(SemiAdapterError::MissingArg { name: "permitted[]" })?
                    .iter()
                    .map(parse_token_amount)
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                vec![parse_token_amount(permitted)?]
            };
            let witness = m.get("witness").and_then(|v| v.as_str()).map(String::from);
            let witness_type = if req.primary_type.contains("Witness") {
                Some(format!("EIP712 witness type for {}", req.primary_type))
            } else {
                None
            };
            Ok(SignSemantic::Permit2TransferFrom {
                spender,
                transfers,
                nonce,
                witness,
                witness_type_string: witness_type,
            })
        }
        _ => Ok(SignSemantic::Other {
            types_json: req.types.clone(),
            message_json: req.message.clone(),
        }),
    }
}

fn decode_eip2612(req: &TypedDataRequest) -> Result<SignSemantic, SemiAdapterError> {
    let m = &req.message;
    let owner = parse_addr(m.get("owner"), "owner")?;
    let spender = parse_addr(m.get("spender"), "spender")?;
    let value = m
        .get("value")
        .and_then(|v| v.as_str())
        .map(|s| amount_with_unlimited_check(s.to_string()))
        .ok_or(SemiAdapterError::MissingArg { name: "value" })?;
    let nonce = m
        .get("nonce")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| m.get("nonce").and_then(|v| v.as_u64()).map(|n| n.to_string()))
        .ok_or(SemiAdapterError::MissingArg { name: "nonce" })?;
    let token = req.domain.verifying_contract.ok_or(SemiAdapterError::MissingArg {
        name: "domain.verifyingContract",
    })?;
    Ok(SignSemantic::Eip2612Permit {
        token,
        owner,
        spender,
        value,
        nonce,
    })
}

fn decode_safe_tx(req: &TypedDataRequest) -> Result<SignSemantic, SemiAdapterError> {
    let m = &req.message;
    let safe = req.domain.verifying_contract.ok_or(SemiAdapterError::MissingArg {
        name: "domain.verifyingContract",
    })?;
    let to = parse_addr(m.get("to"), "to")?;
    let value = m
        .get("value")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "0".into());
    let data = m
        .get("data")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "0x".into());
    let operation = m
        .get("operation")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;
    let nonce = m
        .get("nonce")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| m.get("nonce").and_then(|v| v.as_u64()).map(|n| n.to_string()))
        .unwrap_or_else(|| "0".into());
    Ok(SignSemantic::SafeTx { safe, to, value, data, operation, nonce })
}

fn extract_deadline(req: &TypedDataRequest) -> Option<u64> {
    let m = &req.message;
    if let Some(d) = m.get("deadline").and_then(|v| v.as_u64()) {
        return Some(d);
    }
    if let Some(d) = m.get("sigDeadline").and_then(|v| v.as_u64()) {
        return Some(d);
    }
    if let Some(s) = m.get("deadline").and_then(|v| v.as_str()) {
        return s.parse().ok();
    }
    if let Some(s) = m.get("sigDeadline").and_then(|v| v.as_str()) {
        return s.parse().ok();
    }
    None
}

fn parse_addr(v: Option<&Value>, name: &'static str) -> Result<Address, SemiAdapterError> {
    let s = v
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name })?;
    s.parse().map_err(|_| SemiAdapterError::BadAddress { value: s.into() })
}

fn parse_token_amount(v: &Value) -> Result<TokenAmount, SemiAdapterError> {
    let token = parse_addr(v.get("token"), "permitted.token")?;
    let amount_str = v
        .get("amount")
        .and_then(|s| s.as_str())
        .map(String::from)
        .ok_or(SemiAdapterError::MissingArg { name: "permitted.amount" })?;
    Ok(TokenAmount {
        token,
        amount: AmountSpec {
            raw: amount_str,
            kind: crate::types::AmountKind::Exact,
        },
    })
}

fn parse_token_amount_with_expiry(v: &Value) -> Result<TokenAmountWithExpiry, SemiAdapterError> {
    let token = parse_addr(v.get("token"), "details.token")?;
    let amount_str = v
        .get("amount")
        .and_then(|s| s.as_str())
        .map(String::from)
        .ok_or(SemiAdapterError::MissingArg { name: "details.amount" })?;
    let expiration = v.get("expiration").and_then(|s| s.as_u64());
    Ok(TokenAmountWithExpiry {
        token,
        amount: amount_with_unlimited_check(amount_str),
        expiration,
    })
}

fn parse_uint_string_field(
    m: &Value,
    name: &'static str,
    parent_obj: &str,
) -> Result<String, SemiAdapterError> {
    if let Some(s) = m.get(name).and_then(|v| v.as_str()) {
        return Ok(s.to_string());
    }
    if let Some(n) = m.get(name).and_then(|v| v.as_u64()) {
        return Ok(n.to_string());
    }
    // PermitSingle은 details.nonce
    if let Some(parent) = m.get(parent_obj) {
        if let Some(s) = parent.get(name).and_then(|v| v.as_str()) {
            return Ok(s.to_string());
        }
        if let Some(n) = parent.get(name).and_then(|v| v.as_u64()) {
            return Ok(n.to_string());
        }
    }
    Err(SemiAdapterError::MissingArg { name: "nonce" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decode_eip2612_permit() {
        let req = TypedDataRequest {
            method: "eth_signTypedData_v4".into(),
            signer: "0x1111111111111111111111111111111111111111".parse().unwrap(),
            chain_id: 1,
            domain: Eip712Domain {
                name: Some("USD Coin".into()),
                version: Some("2".into()),
                chain_id: Some(1),
                verifying_contract: Some(
                    "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap(),
                ),
                salt: None,
            },
            primary_type: "Permit".into(),
            message: json!({
                "owner": "0x1111111111111111111111111111111111111111",
                "spender": "0x2222222222222222222222222222222222222222",
                "value": "1000000000",
                "nonce": "0",
                "deadline": "1762500000"
            }),
            types: json!({"Permit": []}),
        };
        let fields = build_sign_fields(&req, Some(1_762_499_000)).unwrap();
        assert!(matches!(fields.semantic, SignSemantic::Eip2612Permit { .. }));
        assert_eq!(fields.deadlines.deadline, Some(1762500000));
    }
}
