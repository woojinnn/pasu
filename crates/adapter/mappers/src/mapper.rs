//! Mapper trait — DecodedCall → ActionEnvelope[].

use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MapperId(pub String);

impl MapperId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapperMatchKey {
    pub decoder_id: abi_resolver::DecoderId,
}

pub struct MapContext<'a> {
    pub chain_id: u64,
    pub from: &'a policy_engine::action::Address,
    pub to: &'a policy_engine::action::Address,
    pub value_wei: &'a policy_engine::action::DecimalString,
    pub block_timestamp: Option<u64>,
    pub token_registry: &'a dyn crate::token_registry::TokenRegistry,
}

#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error("missing argument {0}")]
    MissingArgument(String),
    #[error("unexpected argument type for {name}: {message}")]
    ArgumentMismatch { name: String, message: String },
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

pub trait Mapper: Send + Sync {
    fn id(&self) -> MapperId;
    fn accepts(&self, decoded: &abi_resolver::DecodedCall) -> bool;
    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &abi_resolver::DecodedCall,
    ) -> Result<Vec<policy_engine::ActionEnvelope>, MapperError>;
}

pub trait MapperRegistry: Send + Sync {
    fn resolve(&self, key: &MapperMatchKey) -> Option<Arc<dyn Mapper>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mapper_match_key_serde_roundtrip() {
        let value = json!({
            "decoderId": "uniswap-v2/swap",
        });

        let key: MapperMatchKey = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            key.decoder_id,
            abi_resolver::DecoderId::new("uniswap-v2/swap")
        );

        assert_eq!(serde_json::to_value(key).unwrap(), value);
    }
}
