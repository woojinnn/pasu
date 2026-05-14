//! Revoke action.

use serde::{Deserialize, Serialize};

use crate::action::common::Address;

use super::{ContractRef, RevokeKind};

/// Voluntarily revoke previously granted lending authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeAction {
    /// Contract or token whose authority is being revoked, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<ContractRef>,
    /// Account calling the revoke flow.
    pub caller: Address,
    /// Account whose grant is being renounced or revoked.
    pub subject: Address,
    /// Revocation variant.
    pub revoke_kind: RevokeKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::test_support::{address, assert_json_roundtrip, contract_ref};
    use serde_json::json;

    #[test]
    fn test_revoke_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<RevokeAction>(json!({
            "caller": address(0x70),
            "subject": address(0x71),
            "revokeKind": "erc20_allowance"
        }));
    }

    #[test]
    fn test_revoke_action_serde_roundtrip_full() {
        assert_json_roundtrip::<RevokeAction>(json!({
            "target": contract_ref(),
            "caller": address(0x70),
            "subject": address(0x71),
            "revokeKind": "position_manager_role"
        }));
    }
}
