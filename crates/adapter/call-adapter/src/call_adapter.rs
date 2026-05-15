use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallAdapterId(pub String);

impl CallAdapterId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct CallContext<'a> {
    pub chain_id: u64,
    pub from: &'a policy_engine::action::Address,
    pub to: &'a policy_engine::action::Address,
    pub value_wei: &'a policy_engine::action::DecimalString,
    pub block_timestamp: Option<u64>,
    pub token_registry: &'a dyn mappers::TokenRegistry,
    pub decoder_registry: &'a dyn abi_resolver::DecoderRegistry,
    pub mapper_registry: &'a dyn mappers::MapperRegistry,
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("calldata too short ({0} bytes)")]
    CalldataTooShort(usize),
    #[error("no decoder registered for selector")]
    NoDecoder,
    #[error("no mapper registered for decoder {0}")]
    NoMapper(String),
    #[error("decoder error: {0}")]
    Decoder(#[from] abi_resolver::DecoderError),
    #[error("mapper error: {0}")]
    Mapper(#[from] mappers::MapperError),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// CallAdapter - top of the calldata pipeline. `build()` consumes raw calldata
/// and emits a list of ActionEnvelope. Symmetric to SignAdapter.
pub trait CallAdapter: Send + Sync {
    fn id(&self) -> CallAdapterId;

    /// Match keys this adapter handles. request-router queries the registry
    /// to find the right CallAdapter for an incoming (chain_id, to, selector).
    fn match_keys(&self) -> Vec<abi_resolver::CallMatchKey>;

    fn build(
        &self,
        ctx: &CallContext<'_>,
        calldata: &[u8],
    ) -> Result<Vec<policy_engine::ActionEnvelope>, AdapterError>;
}

pub trait CallAdapterRegistry: Send + Sync {
    fn resolve(&self, key: &abi_resolver::CallMatchKey) -> Option<Arc<dyn CallAdapter>>;
}

#[cfg(test)]
mod tests {
    use super::CallAdapterId;

    #[test]
    fn test_call_adapter_id_roundtrip() {
        let id = CallAdapterId("foo".to_owned());

        let json = serde_json::to_string(&id).unwrap();
        let decoded: CallAdapterId = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, id);
    }
}
