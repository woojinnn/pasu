use std::str::FromStr;

use policy_engine::action::misc::{PermitAction, PermitKind};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity, ValiditySource,
};
use serde_json::{Map, Value};

use crate::{
    SignAdapter, SignAdapterError, SignAdapterId, SignContext, SignMatchKey, SignPayload,
    SignRequest,
};

const ADAPTER_ID: &str = "eip2612/permit@0.2.0";
const PRIMARY_TYPE: &str = "Permit";
const CHAIN_IDS: [u64; 5] = [1, 8453, 10, 42161, 137];
const UINT256_MAX_DEC: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

/// EIP-2612 Permit EIP-712 sign adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Eip2612Adapter;

impl Eip2612Adapter {
    /// Construct an EIP-2612 adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SignAdapter for Eip2612Adapter {
    fn id(&self) -> SignAdapterId {
        SignAdapterId::new(ADAPTER_ID)
    }

    fn match_keys(&self) -> Vec<SignMatchKey> {
        CHAIN_IDS
            .into_iter()
            .map(|chain_id| SignMatchKey {
                chain_id,
                verifying_contract: None,
                primary_type: PRIMARY_TYPE.to_owned(),
            })
            .collect()
    }

    fn build(
        &self,
        ctx: &SignContext<'_>,
        sig: &SignRequest,
    ) -> Result<Vec<ActionEnvelope>, SignAdapterError> {
        let SignPayload::TypedData(typed_data) = &sig.payload else {
            return Err(SignAdapterError::UnsupportedSchema);
        };
        let root = object(typed_data, "typedData")?;
        let primary_type = string_field(root, "primaryType")?;
        if primary_type != PRIMARY_TYPE {
            return Err(SignAdapterError::UnsupportedSchema);
        }
        validate_schema(root)?;

        let domain = object_field(root, "domain")?;
        let verifying_contract = address_field(domain, "verifyingContract")?;
        let message = object_field(root, "message")?;
        let owner = address_field(message, "owner")?;
        if owner != *ctx.signer {
            return Err(SignAdapterError::InvalidTypedData(format!(
                "message.owner {owner} does not match signer {}",
                ctx.signer
            )));
        }
        let spender = address_field(message, "spender")?;
        let value = decimal_field(message, "value")?;
        let _nonce = decimal_field(message, "nonce")?;
        let deadline = decimal_field(message, "deadline")?;

        let amount = permit_amount(&value, UINT256_MAX_DEC);
        let action = Action::Permit(PermitAction {
            permit_kind: PermitKind::Eip2612,
            token: erc20(ctx.chain_id, verifying_contract),
            owner,
            spender: Some(spender),
            spender_label: None,
            recipient: None,
            amount,
            requested_amount: None,
            validity: signature_deadline(deadline),
            signature_validity: None,
        });

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action,
        }])
    }
}

fn validate_schema(root: &Map<String, Value>) -> Result<(), SignAdapterError> {
    let domain = object_field(root, "domain")?;
    let _name = string_field(domain, "name")?;
    let _version = string_field(domain, "version")?;
    let _chain_id = decimal_field(domain, "chainId")?;
    let _verifying_contract = address_field(domain, "verifyingContract")?;
    let types = object_field(root, "types")?;
    validate_type_fields(
        types,
        "EIP712Domain",
        &[
            ("name", "string"),
            ("version", "string"),
            ("chainId", "uint256"),
            ("verifyingContract", "address"),
        ],
    )?;
    validate_type_fields(
        types,
        PRIMARY_TYPE,
        &[
            ("owner", "address"),
            ("spender", "address"),
            ("value", "uint256"),
            ("nonce", "uint256"),
            ("deadline", "uint256"),
        ],
    )
}

fn validate_type_fields(
    types: &Map<String, Value>,
    type_name: &str,
    expected: &[(&str, &str)],
) -> Result<(), SignAdapterError> {
    let fields = types
        .get(type_name)
        .and_then(Value::as_array)
        .ok_or_else(|| SignAdapterError::MissingField(format!("types.{type_name}")))?;
    if fields.len() != expected.len() {
        return Err(SignAdapterError::InvalidTypedData(format!(
            "types.{type_name} field count mismatch"
        )));
    }

    for (field, (expected_name, expected_type)) in fields.iter().zip(expected.iter()) {
        let field = object(field, type_name)?;
        let actual_name = string_field(field, "name")?;
        let actual_type = string_field(field, "type")?;
        if actual_name != *expected_name || actual_type != *expected_type {
            return Err(SignAdapterError::InvalidTypedData(format!(
                "types.{type_name} must include {expected_name}:{expected_type}"
            )));
        }
    }
    Ok(())
}

fn permit_amount(value: &DecimalString, unlimited_value: &str) -> AmountConstraint {
    if value.to_string() == unlimited_value {
        AmountConstraint {
            kind: AmountKind::Unlimited,
            value: None,
        }
    } else {
        AmountConstraint {
            kind: AmountKind::Exact,
            value: Some(value.clone()),
        }
    }
}

fn erc20(chain_id: u64, address: Address) -> AssetRef {
    AssetRef {
        kind: AssetKind::Erc20,
        chain_id,
        address: Some(address),
        symbol: None,
        decimals: None,
    }
}

fn signature_deadline(expires_at: DecimalString) -> Validity {
    Validity {
        expires_at,
        source: ValiditySource::SignatureDeadline,
    }
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, SignAdapterError> {
    value
        .as_object()
        .ok_or_else(|| SignAdapterError::InvalidTypedData(format!("{label} must be an object")))
}

fn object_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a Map<String, Value>, SignAdapterError> {
    object
        .get(field)
        .ok_or_else(|| SignAdapterError::MissingField(field.to_owned()))
        .and_then(|value| self::object(value, field))
}

fn string_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, SignAdapterError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| SignAdapterError::MissingField(field.to_owned()))
}

fn stringish_field(object: &Map<String, Value>, field: &str) -> Result<String, SignAdapterError> {
    let value = object
        .get(field)
        .ok_or_else(|| SignAdapterError::MissingField(field.to_owned()))?;
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(SignAdapterError::InvalidTypedData(format!(
            "{field} must be a string or number"
        ))),
    }
}

fn address_field(object: &Map<String, Value>, field: &str) -> Result<Address, SignAdapterError> {
    let value = stringish_field(object, field)?;
    Address::from_str(&value).map_err(|err| {
        SignAdapterError::InvalidTypedData(format!("{field} must be address: {err}"))
    })
}

fn decimal_field(
    object: &Map<String, Value>,
    field: &str,
) -> Result<DecimalString, SignAdapterError> {
    let value = stringish_field(object, field)?;
    let canonical = canonical_decimal(&value);
    DecimalString::from_str(&canonical).map_err(|err| {
        SignAdapterError::InvalidTypedData(format!("{field} must be uint256 decimal: {err}"))
    })
}

fn canonical_decimal(value: &str) -> String {
    let trimmed = value.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use mappers::EmptyTokenRegistry;
    use policy_engine::action::misc::PermitKind;
    use policy_engine::action::{Action, Address, AmountKind, Category, ValiditySource};
    use serde_json::Value;

    use super::Eip2612Adapter;
    use crate::{parse_sign_request, SignAdapter, SignAdapterError, SignContext, SignPayload};

    const MAX_U256: &str =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";

    fn fixture_request() -> crate::SignRequest {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../integration-tests/data/golden/inputs/eip2612_permit.json"
        ))
        .unwrap();
        let method = fixture["rpc"]["method"].as_str().unwrap();
        let params = &fixture["rpc"]["params"];
        let chain_id = fixture["chain_id"].as_u64().unwrap();
        parse_sign_request(method, params, chain_id).unwrap()
    }

    fn build(
        request: &crate::SignRequest,
    ) -> Result<Vec<policy_engine::ActionEnvelope>, SignAdapterError> {
        let adapter = Eip2612Adapter::new();
        let signer = request.signer.parse::<Address>().unwrap();
        let token_registry = EmptyTokenRegistry;
        let ctx = SignContext {
            chain_id: request.chain_id,
            signer: &signer,
            block_timestamp: None,
            token_registry: &token_registry,
        };
        adapter.build(&ctx, request)
    }

    #[test]
    fn test_eip2612_build_emits_permit_envelope() {
        let request = fixture_request();

        let envelopes = build(&request).unwrap();

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, Category::Misc);
        let Action::Permit(action) = &envelopes[0].action else {
            panic!("expected Action::Permit");
        };
        assert_eq!(action.permit_kind, PermitKind::Eip2612);
        assert_eq!(
            action.owner,
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .parse::<Address>()
                .unwrap()
        );
        assert_eq!(
            action.spender,
            Some(
                "0x1111111111111111111111111111111111111111"
                    .parse::<Address>()
                    .unwrap()
            )
        );
        assert_eq!(
            action.token.address,
            Some(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<Address>()
                    .unwrap()
            )
        );
        assert_eq!(action.token.chain_id, 1);
        assert_eq!(action.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.amount.value.as_ref().map(ToString::to_string),
            Some("50000000".to_owned())
        );
        assert_eq!(action.validity.expires_at.to_string(), "1600");
        assert_eq!(action.validity.source, ValiditySource::SignatureDeadline);
        assert_eq!(action.signature_validity, None);
    }

    #[test]
    fn test_eip2612_unlimited_amount() {
        let mut request = fixture_request();
        let SignPayload::TypedData(typed_data) = &mut request.payload else {
            panic!("expected typed data");
        };
        typed_data["message"]["value"] = Value::String(MAX_U256.to_owned());

        let envelopes = build(&request).unwrap();

        let Action::Permit(action) = &envelopes[0].action else {
            panic!("expected Action::Permit");
        };
        assert_eq!(action.amount.kind, AmountKind::Unlimited);
        assert_eq!(action.amount.value, None);
    }

    #[test]
    fn test_eip2612_rejects_wrong_primary_type() {
        let mut request = fixture_request();
        let SignPayload::TypedData(typed_data) = &mut request.payload else {
            panic!("expected typed data");
        };
        typed_data["primaryType"] = Value::String("Approval".to_owned());

        let err = build(&request).unwrap_err();

        assert!(matches!(err, SignAdapterError::UnsupportedSchema));
    }
}
