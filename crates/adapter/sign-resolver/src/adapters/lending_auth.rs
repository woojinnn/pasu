//! Lending sign-authorization adapter.
//!
//! Three protocols share the same EIP-712 shape — `Authorization` /
//! `DelegationWithSig` — and we discriminate by `(verifyingContract,
//! primaryType)`:
//!
//!   - **Morpho Blue** — `Authorization(authorizer, authorized, isAuthorized,
//!     nonce, deadline)` at a fixed verifyingContract.
//!   - **Compound V3** — `Authorization(owner, manager, isAllowed, nonce,
//!     expiry)` against a Comet market (one verifyingContract per market).
//!   - **Aave V3** — `DelegationWithSig(delegatee, value, nonce, deadline)`
//!     against a variable-debt token. Many verifyingContracts; we register
//!     a wildcard (verifyingContract = None) keyed only on primaryType.
//!
//! The legacy decoder_db branch wired this through `mappers/sign/lending_auth.rs`.
//! Restoring it here so request-router can re-dispatch lending sign requests.

use std::str::FromStr as _;

use policy_engine::action::lending::{
    ContractRef, SignAuthorizationAction, SignAuthorizationScope,
};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, Category, DecimalString,
    Validity, ValiditySource,
};
use serde_json::Value;

use crate::sign_adapter::{
    SignAdapter, SignAdapterError, SignAdapterId, SignContext, SignMatchKey,
};
use crate::SignPayload;
use crate::SignRequest;

const ADAPTER_ID: &str = "sign/lending-auth";

/// Morpho Blue singleton verifyingContract (mainnet).
const MORPHO_BLUE_LC: &str = "0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb";

/// Mainnet Comet markets (Compound V3). Each underlying-asset market is its
/// own contract and serves as the EIP-712 verifyingContract. Lowercase.
const COMPOUND_V3_MARKETS_LC: &[(&str, &str)] = &[
    (
        "0xc3d688b66703497daa19211eedff47f25384cdc3",
        "Compound V3 — cUSDCv3",
    ),
    (
        "0xa17581a9e3356d9a858b789d68b4d866e593ae94",
        "Compound V3 — cWETHv3",
    ),
    (
        "0xa5edbdd9646f8dff606d7448e414884c7d905dca",
        "Compound V3 — cUSDTv3",
    ),
];

const PRIMARY_AUTHORIZATION: &str = "Authorization";
const PRIMARY_DELEGATION_WITH_SIG: &str = "DelegationWithSig";

/// `2^256 - 1` as decimal — Aave allowance "unlimited" sentinel.
const UINT256_MAX_DEC: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

#[derive(Debug, Clone, Default)]
pub struct LendingAuthSignAdapter;

impl LendingAuthSignAdapter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SignAdapter for LendingAuthSignAdapter {
    fn id(&self) -> SignAdapterId {
        SignAdapterId::new(ADAPTER_ID)
    }

    fn match_keys(&self) -> Vec<SignMatchKey> {
        let mut out = Vec::new();
        // Morpho Blue: fixed verifyingContract + Authorization
        out.push(SignMatchKey {
            chain_id: 1,
            verifying_contract: Some(addr(MORPHO_BLUE_LC)),
            primary_type: PRIMARY_AUTHORIZATION.to_owned(),
        });
        // Compound V3 markets: each Comet address + Authorization
        for (market_addr, _) in COMPOUND_V3_MARKETS_LC {
            out.push(SignMatchKey {
                chain_id: 1,
                verifying_contract: Some(addr(market_addr)),
                primary_type: PRIMARY_AUTHORIZATION.to_owned(),
            });
        }
        // Aave V3 debt delegation: wildcard on verifyingContract
        // (each variable-debt token has its own address)
        out.push(SignMatchKey {
            chain_id: 1,
            verifying_contract: None,
            primary_type: PRIMARY_DELEGATION_WITH_SIG.to_owned(),
        });
        out
    }

    fn build(
        &self,
        ctx: &SignContext<'_>,
        sig: &SignRequest,
    ) -> Result<Vec<ActionEnvelope>, SignAdapterError> {
        let SignPayload::TypedData(typed_data) = &sig.payload else {
            return Err(SignAdapterError::UnsupportedSchema);
        };
        let domain = object_field(typed_data, "domain")?;
        let message = object_field(typed_data, "message")?;
        let primary_type = string_field(typed_data, "primaryType")?;
        let verifying_contract = domain
            .get("verifyingContract")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase());

        let envelope = match (primary_type.as_str(), verifying_contract.as_deref()) {
            (PRIMARY_AUTHORIZATION, Some(vc)) if vc == MORPHO_BLUE_LC => {
                morpho_authorization(ctx, message)?
            }
            (PRIMARY_AUTHORIZATION, Some(vc))
                if compound_market_label(vc).is_some() =>
            {
                compound_authorization(ctx, vc, message)?
            }
            (PRIMARY_DELEGATION_WITH_SIG, Some(vc)) => aave_delegation(ctx, vc, message)?,
            _ => return Err(SignAdapterError::UnsupportedSchema),
        };

        Ok(vec![envelope])
    }
}

/// Morpho Blue `Authorization(authorizer, authorized, isAuthorized, nonce, deadline)`.
fn morpho_authorization(
    _ctx: &SignContext<'_>,
    message: &Value,
) -> Result<ActionEnvelope, SignAdapterError> {
    let authorizer = address_field(message, "authorizer")?;
    let authorized = address_field(message, "authorized")?;
    let is_authorized = bool_field(message, "isAuthorized")?;
    let nonce = decimal_field(message, "nonce")?;
    let deadline = decimal_field(message, "deadline")?;

    Ok(ActionEnvelope {
        category: Category::Lending,
        action: Action::SignAuthorization(SignAuthorizationAction {
            market: Some(ContractRef {
                address: Some(addr(MORPHO_BLUE_LC)),
                label: Some("Morpho Blue".into()),
            }),
            authorizer,
            authorized,
            is_authorized,
            authorization_scope: SignAuthorizationScope::All,
            amount: None,
            nonce: Some(nonce),
            validity: Validity {
                expires_at: deadline,
                source: ValiditySource::SignatureDeadline,
            },
        }),
    })
}

/// Compound V3 `Authorization(owner, manager, isAllowed, nonce, expiry)`.
fn compound_authorization(
    _ctx: &SignContext<'_>,
    comet_lc: &str,
    message: &Value,
) -> Result<ActionEnvelope, SignAdapterError> {
    let owner = address_field(message, "owner")?;
    let manager = address_field(message, "manager")?;
    let is_allowed = bool_field(message, "isAllowed")?;
    let nonce = decimal_field(message, "nonce")?;
    let expiry = decimal_field(message, "expiry")?;
    let label = compound_market_label(comet_lc).map(str::to_string);

    Ok(ActionEnvelope {
        category: Category::Lending,
        action: Action::SignAuthorization(SignAuthorizationAction {
            market: Some(ContractRef {
                address: Some(addr(comet_lc)),
                label,
            }),
            authorizer: owner,
            authorized: manager,
            is_authorized: is_allowed,
            authorization_scope: SignAuthorizationScope::ManagerRole,
            amount: None,
            nonce: Some(nonce),
            validity: Validity {
                expires_at: expiry,
                source: ValiditySource::SignatureDeadline,
            },
        }),
    })
}

/// Aave V3 `DelegationWithSig(delegatee, value, nonce, deadline)`.
/// `verifyingContract` is the variable-debt token. The signer (= ctx.signer)
/// is the authorizer; only the delegatee is named in the message.
fn aave_delegation(
    ctx: &SignContext<'_>,
    debt_token_lc: &str,
    message: &Value,
) -> Result<ActionEnvelope, SignAdapterError> {
    let delegatee = address_field(message, "delegatee")?;
    let value = decimal_field(message, "value")?;
    let nonce = decimal_field(message, "nonce")?;
    let deadline = decimal_field(message, "deadline")?;

    Ok(ActionEnvelope {
        category: Category::Lending,
        action: Action::SignAuthorization(SignAuthorizationAction {
            market: Some(ContractRef {
                address: Some(addr(debt_token_lc)),
                label: Some("Aave V3 — debt delegation".into()),
            }),
            authorizer: ctx.signer.clone(),
            authorized: delegatee,
            is_authorized: true,
            authorization_scope: SignAuthorizationScope::DebtOnly,
            amount: Some(amount_or_unlimited(&value)),
            nonce: Some(nonce),
            validity: Validity {
                expires_at: deadline,
                source: ValiditySource::SignatureDeadline,
            },
        }),
    })
}

fn compound_market_label(addr_lc: &str) -> Option<&'static str> {
    COMPOUND_V3_MARKETS_LC
        .iter()
        .find(|(a, _)| *a == addr_lc)
        .map(|(_, l)| *l)
}

fn amount_or_unlimited(value: &DecimalString) -> AmountConstraint {
    if value.to_string() == UINT256_MAX_DEC {
        AmountConstraint {
            kind: AmountKind::Unlimited,
            value: None,
        }
    } else {
        AmountConstraint {
            kind: AmountKind::Max,
            value: Some(value.clone()),
        }
    }
}

// ── tiny JSON helpers ───────────────────────────────────────────────────────

fn object_field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, SignAdapterError> {
    value
        .get(name)
        .filter(|v| v.is_object())
        .ok_or_else(|| SignAdapterError::MissingField(name.to_owned()))
}

fn string_field(value: &Value, name: &str) -> Result<String, SignAdapterError> {
    value
        .get(name)
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| SignAdapterError::MissingField(name.to_owned()))
}

fn address_field(value: &Value, name: &str) -> Result<Address, SignAdapterError> {
    let raw = value
        .get(name)
        .and_then(|v| v.as_str())
        .ok_or_else(|| SignAdapterError::MissingField(name.to_owned()))?;
    Address::from_str(raw)
        .map_err(|e| SignAdapterError::InvalidTypedData(format!("invalid address {name}: {e}")))
}

fn decimal_field(value: &Value, name: &str) -> Result<DecimalString, SignAdapterError> {
    let raw = match value.get(name) {
        Some(v) if v.is_string() => v.as_str().unwrap().to_owned(),
        Some(v) if v.is_number() => v.to_string(),
        _ => return Err(SignAdapterError::MissingField(name.to_owned())),
    };
    DecimalString::from_str(&raw)
        .map_err(|e| SignAdapterError::InvalidTypedData(format!("invalid decimal {name}: {e}")))
}

fn bool_field(value: &Value, name: &str) -> Result<bool, SignAdapterError> {
    value
        .get(name)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| SignAdapterError::MissingField(name.to_owned()))
}

fn addr(lc: &str) -> Address {
    Address::from_str(lc).expect("static address must be valid")
}
