use std::collections::HashMap;
use std::sync::Arc;

use crate::mapper::{Mapper, MapperMatchKey, MapperRegistry};

pub struct InMemoryMapperRegistry {
    by_key: HashMap<MapperMatchKey, Arc<dyn Mapper>>,
}

impl InMemoryMapperRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new()
    }

    pub fn builder() -> InMemoryMapperRegistryBuilder {
        InMemoryMapperRegistryBuilder {
            entries: Vec::new(),
        }
    }
}

impl Default for InMemoryMapperRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InMemoryMapperRegistryBuilder {
    entries: Vec<(MapperMatchKey, Arc<dyn Mapper>)>,
}

impl InMemoryMapperRegistryBuilder {
    pub fn register(mut self, key: MapperMatchKey, mapper: Arc<dyn Mapper>) -> Self {
        self.entries.push((key, mapper));
        self
    }

    pub fn build(self) -> InMemoryMapperRegistry {
        let mut by_key = HashMap::new();
        for (k, m) in self.entries {
            if let Some(existing) = by_key.insert(k.clone(), m.clone()) {
                panic!(
                    "duplicate mapper match key {:?}: existing={:?} new={:?}",
                    k,
                    existing.id(),
                    m.id()
                );
            }
        }
        InMemoryMapperRegistry { by_key }
    }
}

impl MapperRegistry for InMemoryMapperRegistry {
    fn resolve(&self, key: &MapperMatchKey) -> Option<Arc<dyn Mapper>> {
        self.by_key.get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::InMemoryMapperRegistry;
    use crate::mapper::{
        MapContext, Mapper, MapperError, MapperId, MapperMatchKey, MapperRegistry,
    };

    struct MockMapper {
        id: MapperId,
    }

    impl MockMapper {
        fn new(id: &str) -> Self {
            Self {
                id: MapperId::new(id),
            }
        }
    }

    impl Mapper for MockMapper {
        fn id(&self) -> MapperId {
            self.id.clone()
        }

        fn accepts(&self, _decoded: &abi_resolver::DecodedCall) -> bool {
            true
        }

        fn map(
            &self,
            _ctx: &MapContext<'_>,
            _decoded: &abi_resolver::DecodedCall,
        ) -> Result<Vec<policy_engine::ActionEnvelope>, MapperError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn test_in_memory_mapper_registry_resolves() {
        let key = MapperMatchKey {
            decoder_id: abi_resolver::DecoderId::new("uniswap-v2/swap"),
        };
        let mapper = Arc::new(MockMapper::new("mock"));
        let registry = InMemoryMapperRegistry::builder()
            .register(key.clone(), mapper.clone())
            .build();

        let resolved = registry.resolve(&key).unwrap();

        assert_eq!(resolved.id(), mapper.id());
    }

    #[test]
    fn test_in_memory_mapper_registry_miss() {
        let registered_key = MapperMatchKey {
            decoder_id: abi_resolver::DecoderId::new("uniswap-v2/swap"),
        };
        let missing_key = MapperMatchKey {
            decoder_id: abi_resolver::DecoderId::new("uniswap-v3/swap"),
        };
        let mapper = Arc::new(MockMapper::new("mock"));
        let registry = InMemoryMapperRegistry::builder()
            .register(registered_key, mapper)
            .build();

        assert!(registry.resolve(&missing_key).is_none());
    }

    #[test]
    #[should_panic]
    fn test_in_memory_mapper_registry_panics_on_duplicate_keys() {
        let key = MapperMatchKey {
            decoder_id: abi_resolver::DecoderId::new("uniswap-v2/swap"),
        };

        InMemoryMapperRegistry::builder()
            .register(key.clone(), Arc::new(MockMapper::new("first")))
            .register(key, Arc::new(MockMapper::new("second")))
            .build();
    }
}
