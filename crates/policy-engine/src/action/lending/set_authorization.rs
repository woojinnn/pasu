//! Set-authorization action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint};

use super::{AuthorizationScope, MarketRef};

/// Grant or revoke lending authorization on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAuthorizationAction {
    /// Market or protocol where authorization applies, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Account granting or revoking authority.
    pub authorizer: Address,
    /// Account receiving or losing authority.
    pub authorized: Address,
    /// Whether authority is granted.
    pub is_authorized: bool,
    /// Authorization scope.
    pub authorization_scope: AuthorizationScope,
    /// Delegation amount cap, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountConstraint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::test_support::{address, amount, assert_json_roundtrip, market};
    use serde_json::json;

    #[test]
    fn test_set_authorization_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<SetAuthorizationAction>(json!({
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": true,
            "authorizationScope": "all"
        }));
    }

    #[test]
    fn test_set_authorization_action_serde_roundtrip_full() {
        assert_json_roundtrip::<SetAuthorizationAction>(json!({
            "market": market(),
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": false,
            "authorizationScope": "debt_only",
            "amount": amount("unlimited", "0")
        }));
    }
}
