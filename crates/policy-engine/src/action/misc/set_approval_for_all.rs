//! Set-approval-for-all action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRef};

/// Toggle collection-wide NFT operator approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetApprovalForAllAction {
    /// NFT collection whose operator approval changes.
    pub collection: AssetRef,
    /// Operator receiving or losing approval.
    pub operator: Address,
    /// Human-readable operator label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_label: Option<String>,
    /// Whether collection-wide approval is granted.
    pub approved: bool,
    /// Previous approval state, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previously_approved: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, assert_json_roundtrip, erc721};
    use serde_json::json;

    #[test]
    fn test_set_approval_for_all_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<SetApprovalForAllAction>(json!({
            "collection": erc721("NFT"),
            "operator": address(0x41),
            "approved": true
        }));
    }

    #[test]
    fn test_set_approval_for_all_action_serde_roundtrip_full() {
        assert_json_roundtrip::<SetApprovalForAllAction>(json!({
            "collection": erc721("NFT"),
            "operator": address(0x41),
            "operatorLabel": "Known Operator",
            "approved": false,
            "previouslyApproved": true
        }));
    }
}
