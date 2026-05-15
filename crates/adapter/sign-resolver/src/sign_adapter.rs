//! SignAdapter - direct signature -> ActionEnvelope[]. Symmetric to
//! CallAdapter (in call-adapter). request-router consumes both via the
//! same build() API.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignAdapterId(pub String);

impl SignAdapterId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignMatchKey {
    pub chain_id: u64,
    /// EIP-712 verifyingContract. None matches any contract (e.g. EIP-2612
    /// Permit is registered with verifying_contract = None since each ERC20
    /// token has its own).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifying_contract: Option<policy_engine::action::Address>,
    /// EIP-712 primaryType, e.g. "Permit", "PermitSingle", "PermitBatch".
    /// For non-typed-data sign payloads (personal_sign, eth_sign) use a
    /// well-known sentinel like "RawMessage" or "RawHash".
    pub primary_type: String,
}

pub struct SignContext<'a> {
    pub chain_id: u64,
    pub signer: &'a policy_engine::action::Address,
    pub block_timestamp: Option<u64>,
    pub token_registry: &'a dyn mappers::TokenRegistry,
}

#[derive(Debug, thiserror::Error)]
pub enum SignAdapterError {
    #[error("unsupported schema")]
    UnsupportedSchema,
    #[error("invalid typed data: {0}")]
    InvalidTypedData(String),
    #[error("missing field {0}")]
    MissingField(String),
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

pub trait SignAdapter: Send + Sync {
    fn id(&self) -> SignAdapterId;

    fn match_keys(&self) -> Vec<SignMatchKey>;

    fn build(
        &self,
        ctx: &SignContext<'_>,
        sig: &crate::SignRequest,
    ) -> Result<Vec<policy_engine::ActionEnvelope>, SignAdapterError>;
}

pub trait SignAdapterRegistry: Send + Sync {
    fn resolve(&self, key: &SignMatchKey) -> Option<Arc<dyn SignAdapter>>;
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::SignMatchKey;

    fn address(value: &str) -> policy_engine::action::Address {
        policy_engine::action::Address::from_str(value).unwrap()
    }

    #[test]
    fn test_sign_match_key_serde_roundtrip() {
        let key = SignMatchKey {
            chain_id: 1,
            verifying_contract: Some(address("0x1111111111111111111111111111111111111111")),
            primary_type: "Permit".to_owned(),
        };

        let json = serde_json::to_value(&key).unwrap();
        assert_eq!(json["chainId"], 1);
        assert_eq!(
            json["verifyingContract"],
            "0x1111111111111111111111111111111111111111"
        );
        assert_eq!(json["primaryType"], "Permit");

        let decoded: SignMatchKey = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_sign_match_key_wildcard_verifying_contract() {
        let key = SignMatchKey {
            chain_id: 1,
            verifying_contract: None,
            primary_type: "Permit".to_owned(),
        };

        let json = serde_json::to_value(&key).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "chainId": 1,
                "primaryType": "Permit",
            })
        );

        let decoded: SignMatchKey = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, key);
    }
}
