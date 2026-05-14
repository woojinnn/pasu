//! Sign-authorization action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, DecimalString, Validity};

use super::{ContractRef, SignAuthorizationScope};

/// Sign a lending authorization payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignAuthorizationAction {
    /// Market or verifying contract, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<ContractRef>,
    /// Account signing the authorization.
    pub authorizer: Address,
    /// Account receiving or losing authority.
    pub authorized: Address,
    /// Whether authority is granted.
    pub is_authorized: bool,
    /// Signed authorization scope.
    pub authorization_scope: SignAuthorizationScope,
    /// Delegation amount cap, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountConstraint>,
    /// Signature nonce, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<DecimalString>,
    /// Signature validity window.
    pub validity: Validity,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::test_support::{
        address, amount, assert_json_roundtrip, contract_ref, validity,
    };
    use serde_json::json;

    #[test]
    fn test_sign_authorization_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<SignAuthorizationAction>(json!({
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": true,
            "authorizationScope": "all",
            "validity": validity()
        }));
    }

    #[test]
    fn test_sign_authorization_action_serde_roundtrip_full() {
        assert_json_roundtrip::<SignAuthorizationAction>(json!({
            "market": contract_ref(),
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": false,
            "authorizationScope": "debt_only",
            "amount": amount("exact", "1000"),
            "nonce": "7",
            "validity": validity()
        }));
    }
}
