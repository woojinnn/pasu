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

const ADAPTER_ID: &str = "permit2/eip712@0.2.0";
const PERMIT2_ADDRESS: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";
const CHAIN_IDS: [u64; 5] = [1, 8453, 10, 42161, 137];
const PRIMARY_TYPES: [Permit2PrimaryType; 6] = [
    Permit2PrimaryType::PermitSingle,
    Permit2PrimaryType::PermitBatch,
    Permit2PrimaryType::PermitTransferFrom,
    Permit2PrimaryType::PermitBatchTransferFrom,
    Permit2PrimaryType::PermitWitnessTransferFrom,
    Permit2PrimaryType::PermitBatchWitnessTransferFrom,
];
const UINT160_MAX_DEC: &str = "1461501637330902918203684832716283019655932542975";
const UINT256_MAX_DEC: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

/// Permit2 EIP-712 sign adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Permit2Adapter;

impl Permit2Adapter {
    /// Construct a Permit2 adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SignAdapter for Permit2Adapter {
    fn id(&self) -> SignAdapterId {
        SignAdapterId::new(ADAPTER_ID)
    }

    fn match_keys(&self) -> Vec<SignMatchKey> {
        let verifying_contract = permit2_address();
        CHAIN_IDS
            .into_iter()
            .flat_map(|chain_id| {
                PRIMARY_TYPES.into_iter().map({
                    let verifying_contract = verifying_contract.clone();
                    move |primary_type| SignMatchKey {
                        chain_id,
                        verifying_contract: Some(verifying_contract.clone()),
                        primary_type: primary_type.as_str().to_owned(),
                    }
                })
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
        let primary_type = Permit2PrimaryType::from_str(string_field(root, "primaryType")?)
            .ok_or(SignAdapterError::UnsupportedSchema)?;
        validate_domain(root)?;

        let message = object_field(root, "message")?;
        let owner = ctx.signer.clone();
        match primary_type {
            Permit2PrimaryType::PermitSingle => {
                let details = object_field(message, "details")?;
                let approval = approval_from_details(ctx.chain_id, details)?;
                let spender = address_field(message, "spender")?;
                let deadline = decimal_field(message, "sigDeadline")?;
                Ok(vec![permit_envelope(
                    PermitKind::Permit2Single,
                    owner,
                    spender,
                    approval,
                    deadline,
                    AmountKind::Max,
                )])
            }
            Permit2PrimaryType::PermitBatch => {
                let details = array_field(message, "details")?;
                let spender = address_field(message, "spender")?;
                let deadline = decimal_field(message, "sigDeadline")?;
                details
                    .iter()
                    .map(|value| {
                        let details = object(value, "details[]")?;
                        let approval = approval_from_details(ctx.chain_id, details)?;
                        Ok(permit_envelope(
                            PermitKind::Permit2Single,
                            owner.clone(),
                            spender.clone(),
                            approval,
                            deadline.clone(),
                            AmountKind::Max,
                        ))
                    })
                    .collect()
            }
            Permit2PrimaryType::PermitTransferFrom => {
                let permitted = object_field(message, "permitted")?;
                let approval = approval_from_permission(ctx.chain_id, permitted)?;
                let spender = address_field(message, "spender")?;
                let _nonce = decimal_field(message, "nonce")?;
                let deadline = decimal_field(message, "deadline")?;
                Ok(vec![permit_envelope(
                    PermitKind::Permit2Transfer,
                    owner,
                    spender,
                    approval,
                    deadline,
                    AmountKind::Max,
                )])
            }
            Permit2PrimaryType::PermitBatchTransferFrom => {
                batch_transfer_envelopes(ctx.chain_id, owner, message, false)
            }
            Permit2PrimaryType::PermitWitnessTransferFrom => {
                require_witness(message)?;
                let permitted = object_field(message, "permitted")?;
                let approval = approval_from_permission(ctx.chain_id, permitted)?;
                let spender = address_field(message, "spender")?;
                let _nonce = decimal_field(message, "nonce")?;
                let deadline = decimal_field(message, "deadline")?;
                Ok(vec![permit_envelope(
                    PermitKind::Permit2Transfer,
                    owner,
                    spender,
                    approval,
                    deadline,
                    AmountKind::Max,
                )])
            }
            Permit2PrimaryType::PermitBatchWitnessTransferFrom => {
                batch_transfer_envelopes(ctx.chain_id, owner, message, true)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
enum Permit2PrimaryType {
    PermitSingle,
    PermitBatch,
    PermitTransferFrom,
    PermitBatchTransferFrom,
    PermitWitnessTransferFrom,
    PermitBatchWitnessTransferFrom,
}

impl Permit2PrimaryType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PermitSingle => "PermitSingle",
            Self::PermitBatch => "PermitBatch",
            Self::PermitTransferFrom => "PermitTransferFrom",
            Self::PermitBatchTransferFrom => "PermitBatchTransferFrom",
            Self::PermitWitnessTransferFrom => "PermitWitnessTransferFrom",
            Self::PermitBatchWitnessTransferFrom => "PermitBatchWitnessTransferFrom",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "PermitSingle" => Some(Self::PermitSingle),
            "PermitBatch" => Some(Self::PermitBatch),
            "PermitTransferFrom" => Some(Self::PermitTransferFrom),
            "PermitBatchTransferFrom" => Some(Self::PermitBatchTransferFrom),
            "PermitWitnessTransferFrom" => Some(Self::PermitWitnessTransferFrom),
            "PermitBatchWitnessTransferFrom" => Some(Self::PermitBatchWitnessTransferFrom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Approval {
    chain_id: u64,
    token: Address,
    amount: DecimalString,
}

fn batch_transfer_envelopes(
    chain_id: u64,
    owner: Address,
    message: &Map<String, Value>,
    require_witness_field: bool,
) -> Result<Vec<ActionEnvelope>, SignAdapterError> {
    if require_witness_field {
        require_witness(message)?;
    }
    let permitted = array_field(message, "permitted")?;
    let spender = address_field(message, "spender")?;
    let _nonce = decimal_field(message, "nonce")?;
    let deadline = decimal_field(message, "deadline")?;

    permitted
        .iter()
        .map(|value| {
            let permitted = object(value, "permitted[]")?;
            let approval = approval_from_permission(chain_id, permitted)?;
            Ok(permit_envelope(
                PermitKind::Permit2Transfer,
                owner.clone(),
                spender.clone(),
                approval,
                deadline.clone(),
                AmountKind::Max,
            ))
        })
        .collect()
}

fn approval_from_details(
    chain_id: u64,
    details: &Map<String, Value>,
) -> Result<Approval, SignAdapterError> {
    let token = address_field(details, "token")?;
    let amount = decimal_field(details, "amount")?;
    let _expiration = decimal_field(details, "expiration")?;
    let _nonce = decimal_field(details, "nonce")?;
    Ok(Approval {
        chain_id,
        token,
        amount,
    })
}

fn approval_from_permission(
    chain_id: u64,
    permitted: &Map<String, Value>,
) -> Result<Approval, SignAdapterError> {
    let token = address_field(permitted, "token")?;
    let amount = decimal_field(permitted, "amount")?;
    Ok(Approval {
        chain_id,
        token,
        amount,
    })
}

fn permit_envelope(
    permit_kind: PermitKind,
    owner: Address,
    spender: Address,
    approval: Approval,
    deadline: DecimalString,
    default_amount_kind: AmountKind,
) -> ActionEnvelope {
    let action = Action::Permit(PermitAction {
        permit_kind,
        token: erc20(approval.chain_id, approval.token),
        owner,
        spender: Some(spender),
        spender_label: None,
        recipient: None,
        amount: permit_amount(&approval.amount, default_amount_kind),
        requested_amount: None,
        validity: signature_deadline(deadline),
        signature_validity: None,
    });

    ActionEnvelope {
        category: Category::Misc,
        action,
    }
}

fn permit_amount(value: &DecimalString, default_kind: AmountKind) -> AmountConstraint {
    let raw = value.to_string();
    if raw == UINT160_MAX_DEC || raw == UINT256_MAX_DEC {
        AmountConstraint {
            kind: AmountKind::Unlimited,
            value: None,
        }
    } else {
        AmountConstraint {
            kind: default_kind,
            value: Some(value.clone()),
        }
    }
}

fn erc20(_chain_id: u64, address: Address) -> AssetRef {
    AssetRef {
        kind: AssetKind::Erc20,
        address: Some(address),
        token_id: None,
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

fn validate_domain(root: &Map<String, Value>) -> Result<(), SignAdapterError> {
    let domain = object_field(root, "domain")?;
    let verifying_contract = address_field(domain, "verifyingContract")?;
    if verifying_contract != permit2_address() {
        return Err(SignAdapterError::UnsupportedSchema);
    }
    let _chain_id = decimal_field(domain, "chainId")?;
    Ok(())
}

fn require_witness(message: &Map<String, Value>) -> Result<(), SignAdapterError> {
    if message.contains_key("witness") {
        Ok(())
    } else {
        Err(SignAdapterError::MissingField("witness".to_owned()))
    }
}

fn permit2_address() -> Address {
    PERMIT2_ADDRESS.parse().expect("valid Permit2 address")
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

fn array_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a [Value], SignAdapterError> {
    object
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| SignAdapterError::InvalidTypedData(format!("{field} must be an array")))
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

    use super::Permit2Adapter;
    use crate::{parse_sign_request, SignAdapter, SignAdapterError, SignContext, SignPayload};

    fn fixture_request(name: &str) -> crate::SignRequest {
        let raw = match name {
            "permit2_permit_single.json" => include_str!(
                "../../../../integration-tests/data/golden/inputs/permit2_permit_single.json"
            ),
            "permit2_permit_batch.json" => include_str!(
                "../../../../integration-tests/data/golden/inputs/permit2_permit_batch.json"
            ),
            _ => panic!("unknown fixture {name}"),
        };
        let fixture: Value = serde_json::from_str(raw).unwrap();
        let method = fixture["rpc"]["method"].as_str().unwrap();
        let params = &fixture["rpc"]["params"];
        let chain_id = fixture["chain_id"].as_u64().unwrap();
        parse_sign_request(method, params, chain_id).unwrap()
    }

    fn build(
        request: &crate::SignRequest,
    ) -> Result<Vec<policy_engine::ActionEnvelope>, SignAdapterError> {
        let adapter = Permit2Adapter::new();
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
    fn test_permit2_permit_single_build() {
        let request = fixture_request("permit2_permit_single.json");

        let envelopes = build(&request).unwrap();

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, Category::Misc);
        let Action::Permit(action) = &envelopes[0].action else {
            panic!("expected Action::Permit");
        };
        assert_eq!(action.permit_kind, PermitKind::Permit2Single);
        assert_eq!(action.owner, request.signer.parse::<Address>().unwrap());
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
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                    .parse::<Address>()
                    .unwrap()
            )
        );
        assert_eq!(action.amount.kind, AmountKind::Max);
        assert_eq!(
            action.amount.value.as_ref().map(ToString::to_string),
            Some("10000000000000000".to_owned())
        );
        assert_eq!(action.validity.expires_at.to_string(), "1600");
        assert_eq!(action.validity.source, ValiditySource::SignatureDeadline);
    }

    #[test]
    fn test_permit2_permit_batch_emits_n_envelopes() {
        let request = fixture_request("permit2_permit_batch.json");

        let envelopes = build(&request).unwrap();

        assert_eq!(envelopes.len(), 2);
        for envelope in &envelopes {
            assert_eq!(envelope.category, Category::Misc);
            let Action::Permit(action) = &envelope.action else {
                panic!("expected Action::Permit");
            };
            assert_eq!(action.permit_kind, PermitKind::Permit2Single);
            assert_eq!(action.amount.kind, AmountKind::Max);
            assert_eq!(action.validity.expires_at.to_string(), "1600");
        }
        let Action::Permit(first) = &envelopes[0].action else {
            panic!("expected Action::Permit");
        };
        let Action::Permit(second) = &envelopes[1].action else {
            panic!("expected Action::Permit");
        };
        assert_eq!(
            first.token.address,
            Some(
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                    .parse::<Address>()
                    .unwrap()
            )
        );
        assert_eq!(
            second.token.address,
            Some(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<Address>()
                    .unwrap()
            )
        );
    }

    #[test]
    fn test_permit2_rejects_unknown_primary_type() {
        let mut request = fixture_request("permit2_permit_single.json");
        let SignPayload::TypedData(typed_data) = &mut request.payload else {
            panic!("expected typed data");
        };
        typed_data["primaryType"] = Value::String("PermitUnknown".to_owned());

        let err = build(&request).unwrap_err();

        assert!(matches!(err, SignAdapterError::UnsupportedSchema));
    }
}
