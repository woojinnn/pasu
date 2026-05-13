use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use crate::decoder::{CallMatchKey, Decoder, DecoderRegistry};
use policy_engine::action::Address;

pub static WILDCARD_TO: LazyLock<Address> = LazyLock::new(|| {
    "0x0000000000000000000000000000000000000000"
        .parse()
        .expect("wildcard address is valid")
});

pub struct InMemoryDecoderRegistry {
    by_key: HashMap<CallMatchKey, Arc<dyn Decoder>>,
}

impl InMemoryDecoderRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
        }
    }

    pub fn builder() -> InMemoryDecoderRegistryBuilder {
        InMemoryDecoderRegistryBuilder {
            decoders: Vec::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new()
    }
}

impl Default for InMemoryDecoderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InMemoryDecoderRegistryBuilder {
    decoders: Vec<Arc<dyn Decoder>>,
}

impl InMemoryDecoderRegistryBuilder {
    pub fn register(mut self, decoder: Arc<dyn Decoder>) -> Self {
        self.decoders.push(decoder);
        self
    }

    pub fn build(self) -> InMemoryDecoderRegistry {
        let mut by_key = HashMap::new();
        for d in self.decoders {
            for k in d.match_keys() {
                if let Some(existing) = by_key.insert(k.clone(), d.clone()) {
                    panic!(
                        "duplicate decoder match key {:?}: existing={:?} new={:?}",
                        k,
                        existing.id(),
                        d.id()
                    );
                }
            }
        }
        InMemoryDecoderRegistry { by_key }
    }
}

impl DecoderRegistry for InMemoryDecoderRegistry {
    fn resolve(&self, key: &CallMatchKey) -> Option<Arc<dyn Decoder>> {
        if let Some(decoder) = self.by_key.get(key) {
            return Some(decoder.clone());
        }

        let wildcard_key = CallMatchKey {
            chain_id: key.chain_id,
            to: WILDCARD_TO.clone(),
            selector: key.selector,
        };
        self.by_key.get(&wildcard_key).cloned()
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        self.by_key.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{DecodeContext, DecodedCall, DecoderError, DecoderId};
    use std::sync::Arc;

    #[derive(Debug)]
    struct MockDecoder {
        id: DecoderId,
        keys: Vec<CallMatchKey>,
    }

    impl MockDecoder {
        fn new(id: &str, keys: Vec<CallMatchKey>) -> Self {
            Self {
                id: DecoderId::new(id),
                keys,
            }
        }
    }

    impl Decoder for MockDecoder {
        fn id(&self) -> DecoderId {
            self.id.clone()
        }

        fn match_keys(&self) -> Vec<CallMatchKey> {
            self.keys.clone()
        }

        fn decode(
            &self,
            _ctx: &DecodeContext<'_>,
            _calldata: &[u8],
        ) -> Result<DecodedCall, DecoderError> {
            Err(DecoderError::UnsupportedSelector)
        }
    }

    fn key(selector: [u8; 4]) -> CallMatchKey {
        CallMatchKey {
            chain_id: 1,
            to: "0x1111111111111111111111111111111111111111"
                .parse()
                .unwrap(),
            selector,
        }
    }

    fn key_to(selector: [u8; 4], to: &str) -> CallMatchKey {
        CallMatchKey {
            chain_id: 1,
            to: to.parse().unwrap(),
            selector,
        }
    }

    #[test]
    fn test_in_memory_registry_resolves() {
        let key = key([0x38, 0xed, 0x17, 0x39]);
        let registry = InMemoryDecoderRegistry::builder()
            .register(Arc::new(MockDecoder::new("mock", vec![key.clone()])))
            .build();

        let decoder = registry.resolve(&key).expect("decoder should resolve");
        assert_eq!(decoder.id().as_str(), "mock");
    }

    #[test]
    fn test_in_memory_registry_no_match_returns_none() {
        let registered = key([0x38, 0xed, 0x17, 0x39]);
        let missing = key([0x09, 0x5e, 0xa7, 0xb3]);
        let registry = InMemoryDecoderRegistry::builder()
            .register(Arc::new(MockDecoder::new("mock", vec![registered])))
            .build();

        assert!(registry.resolve(&missing).is_none());
    }

    #[test]
    fn test_in_memory_registry_falls_back_to_wildcard_to_after_exact_miss() {
        let selector = [0x09, 0x5e, 0xa7, 0xb3];
        let exact_key = key_to(selector, "0x1111111111111111111111111111111111111111");
        let wildcard_key = key_to(selector, "0x0000000000000000000000000000000000000000");
        let random_token_key = key_to(selector, "0x1234567890123456789012345678901234567890");
        let registry = InMemoryDecoderRegistry::builder()
            .register(Arc::new(MockDecoder::new("wildcard", vec![wildcard_key])))
            .register(Arc::new(MockDecoder::new("exact", vec![exact_key.clone()])))
            .build();

        let wildcard_decoder = registry
            .resolve(&random_token_key)
            .expect("decoder should resolve via wildcard to");
        assert_eq!(wildcard_decoder.id().as_str(), "wildcard");

        let exact_decoder = registry
            .resolve(&exact_key)
            .expect("decoder should resolve via exact key");
        assert_eq!(exact_decoder.id().as_str(), "exact");
    }

    #[test]
    #[should_panic]
    fn test_in_memory_registry_panics_on_duplicate_keys() {
        let key = key([0x38, 0xed, 0x17, 0x39]);
        let _registry = InMemoryDecoderRegistry::builder()
            .register(Arc::new(MockDecoder::new("first", vec![key.clone()])))
            .register(Arc::new(MockDecoder::new("second", vec![key])))
            .build();
    }
}
