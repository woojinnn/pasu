use std::collections::HashMap;
use std::sync::Arc;

use crate::decoder::{CallMatchKey, Decoder, DecoderRegistry};

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
        self.by_key.get(key).cloned()
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
    #[should_panic]
    fn test_in_memory_registry_panics_on_duplicate_keys() {
        let key = key([0x38, 0xed, 0x17, 0x39]);
        let _registry = InMemoryDecoderRegistry::builder()
            .register(Arc::new(MockDecoder::new("first", vec![key.clone()])))
            .register(Arc::new(MockDecoder::new("second", vec![key])))
            .build();
    }
}
