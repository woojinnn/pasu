use std::collections::HashMap;
use std::sync::Arc;

use crate::sign_adapter::{SignAdapter, SignAdapterRegistry, SignMatchKey};

pub struct InMemorySignAdapterRegistry {
    by_key: HashMap<SignMatchKey, Arc<dyn SignAdapter>>,
}

impl InMemorySignAdapterRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new()
    }

    pub fn builder() -> InMemorySignAdapterRegistryBuilder {
        InMemorySignAdapterRegistryBuilder {
            adapters: Vec::new(),
        }
    }
}

impl Default for InMemorySignAdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InMemorySignAdapterRegistryBuilder {
    adapters: Vec<Arc<dyn SignAdapter>>,
}

impl InMemorySignAdapterRegistryBuilder {
    pub fn register(mut self, adapter: Arc<dyn SignAdapter>) -> Self {
        self.adapters.push(adapter);
        self
    }

    pub fn build(self) -> InMemorySignAdapterRegistry {
        let mut by_key = HashMap::new();
        for a in self.adapters {
            for k in a.match_keys() {
                if let Some(existing) = by_key.insert(k.clone(), a.clone()) {
                    panic!(
                        "duplicate sign adapter match key {:?}: existing={:?} new={:?}",
                        k,
                        existing.id(),
                        a.id()
                    );
                }
            }
        }
        InMemorySignAdapterRegistry { by_key }
    }
}

impl SignAdapterRegistry for InMemorySignAdapterRegistry {
    fn resolve(&self, key: &SignMatchKey) -> Option<Arc<dyn SignAdapter>> {
        self.by_key.get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use policy_engine::action::Address;

    use crate::{
        InMemorySignAdapterRegistry, SignAdapter, SignAdapterError, SignAdapterId,
        SignAdapterRegistry, SignContext, SignMatchKey,
    };

    struct MockSignAdapter {
        id: SignAdapterId,
        keys: Vec<SignMatchKey>,
    }

    impl MockSignAdapter {
        fn new(id: &str, keys: Vec<SignMatchKey>) -> Self {
            Self {
                id: SignAdapterId::new(id),
                keys,
            }
        }
    }

    impl SignAdapter for MockSignAdapter {
        fn id(&self) -> SignAdapterId {
            self.id.clone()
        }

        fn match_keys(&self) -> Vec<SignMatchKey> {
            self.keys.clone()
        }

        fn build(
            &self,
            _ctx: &SignContext<'_>,
            _sig: &crate::SignRequest,
        ) -> Result<Vec<policy_engine::ActionEnvelope>, SignAdapterError> {
            Ok(Vec::new())
        }
    }

    fn address(value: &str) -> Address {
        value.parse().unwrap()
    }

    fn key(primary_type: &str) -> SignMatchKey {
        SignMatchKey {
            chain_id: 1,
            verifying_contract: Some(address("0x2222222222222222222222222222222222222222")),
            primary_type: primary_type.to_owned(),
        }
    }

    #[test]
    fn test_in_memory_sign_adapter_registry_resolves() {
        let key = key("Permit");
        let registry = InMemorySignAdapterRegistry::builder()
            .register(Arc::new(MockSignAdapter::new("mock", vec![key.clone()])))
            .build();

        let adapter = registry.resolve(&key).expect("adapter should resolve");

        assert_eq!(adapter.id().as_str(), "mock");
    }

    #[test]
    fn test_in_memory_sign_adapter_registry_miss() {
        let registry = InMemorySignAdapterRegistry::builder()
            .register(Arc::new(MockSignAdapter::new("mock", vec![key("Permit")])))
            .build();

        let missing_key = key("PermitSingle");

        assert!(registry.resolve(&missing_key).is_none());
    }

    #[test]
    #[should_panic]
    fn test_in_memory_sign_adapter_registry_panics_on_duplicate() {
        let key = key("Permit");

        let _registry = InMemorySignAdapterRegistry::builder()
            .register(Arc::new(MockSignAdapter::new("first", vec![key.clone()])))
            .register(Arc::new(MockSignAdapter::new("second", vec![key])))
            .build();
    }
}
