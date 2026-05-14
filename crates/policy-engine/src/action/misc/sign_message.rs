//! Sign-message action.

use serde::{Deserialize, Serialize};

use crate::action::common::Hex;

use super::SignMessageDomain;

/// Sign an EIP-712 message envelope that was not normalized further.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignMessageAction {
    /// EIP-712 domain.
    pub domain: SignMessageDomain,
    /// EIP-712 primary type.
    pub primary_type: String,
    /// Human-readable domain label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_label: Option<String>,
    /// Human-readable primary type label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_type_label: Option<String>,
    /// EIP-712 message digest.
    pub message_digest: Hex,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{assert_json_roundtrip, domain, hex32};
    use serde_json::json;

    #[test]
    fn test_sign_message_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<SignMessageAction>(json!({
            "domain": {},
            "primaryType": "Order",
            "messageDigest": hex32(0x70)
        }));
    }

    #[test]
    fn test_sign_message_action_serde_roundtrip_full() {
        assert_json_roundtrip::<SignMessageAction>(json!({
            "domain": domain(),
            "primaryType": "Order",
            "domainLabel": "Example App",
            "primaryTypeLabel": "Order Signature",
            "messageDigest": hex32(0x70)
        }));
    }
}
