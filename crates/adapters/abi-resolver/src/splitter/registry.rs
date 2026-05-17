//! Splitter registry — lookup by `(chain_id, to, selector)`.
//!
//! Mirrors the CallAdapterRegistry pattern but lives in abi-resolver so the
//! request-router can dispatch to the right splitter before it knows
//! anything about call-adapter (which will be deleted once the refactor
//! completes).
//!
//! Calls whose key doesn't match any registered splitter should fall back
//! to [`super::IdentitySplitter`] — the registry itself does not include the
//! identity splitter, because keying it on every possible address would be
//! impossible; instead, the caller decides "no match → identity".

use std::collections::HashMap;
use std::sync::Arc;

use crate::CallMatchKey;

use super::Splitter;

pub trait SplitterRegistry: Send + Sync {
    /// Look up the splitter registered for `key`. Returns `None` when no
    /// registered splitter matches — the caller is responsible for falling
    /// back to identity in that case.
    fn resolve(&self, key: &CallMatchKey) -> Option<Arc<dyn Splitter>>;

    /// All keys registered with this registry. Used by introspection /
    /// debug tooling; not on the hot path.
    fn match_keys(&self) -> Vec<CallMatchKey>;
}

#[derive(Default)]
pub struct InMemorySplitterRegistry {
    by_key: HashMap<CallMatchKey, Arc<dyn Splitter>>,
}

impl InMemorySplitterRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn builder() -> InMemorySplitterRegistryBuilder {
        InMemorySplitterRegistryBuilder {
            splitters: Vec::new(),
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::new()
    }
}

impl SplitterRegistry for InMemorySplitterRegistry {
    fn resolve(&self, key: &CallMatchKey) -> Option<Arc<dyn Splitter>> {
        self.by_key.get(key).cloned()
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        self.by_key.keys().cloned().collect()
    }
}

pub struct InMemorySplitterRegistryBuilder {
    splitters: Vec<Arc<dyn Splitter>>,
}

impl InMemorySplitterRegistryBuilder {
    #[must_use]
    pub fn register(mut self, splitter: Arc<dyn Splitter>) -> Self {
        self.splitters.push(splitter);
        self
    }

    /// Construct the registry. Panics on duplicate `(chain_id, to, selector)`
    /// keys — there should be exactly one splitter per (router, selector)
    /// pair, mirroring CallAdapterRegistry's invariant.
    #[must_use]
    pub fn build(self) -> InMemorySplitterRegistry {
        let mut by_key: HashMap<CallMatchKey, Arc<dyn Splitter>> = HashMap::new();
        for s in self.splitters {
            for k in s.match_keys() {
                if by_key.insert(k.clone(), s.clone()).is_some() {
                    panic!("duplicate splitter match key {k:?}");
                }
            }
        }
        InMemorySplitterRegistry { by_key }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{SplitContext, SplitError, SubCall};
    use super::*;
    use policy_engine::action::{Address, DecimalString};
    use std::str::FromStr as _;

    struct StubSplitter {
        key: CallMatchKey,
    }

    impl Splitter for StubSplitter {
        fn match_keys(&self) -> Vec<CallMatchKey> {
            vec![self.key.clone()]
        }
        fn split(
            &self,
            ctx: &SplitContext<'_>,
            _calldata: &[u8],
        ) -> Result<Vec<SubCall>, SplitError> {
            Ok(vec![SubCall {
                to: ctx.to.clone(),
                value_wei: ctx.value_wei.clone(),
                calldata: Vec::new(),
                decoded: None,
            }])
        }
    }

    fn addr(s: &str) -> Address {
        Address::from_str(s).unwrap()
    }

    #[test]
    fn registry_resolves_registered_key() {
        let key = CallMatchKey {
            chain_id: 1,
            to: addr("0x1111111111111111111111111111111111111111"),
            selector: [0xaa, 0xbb, 0xcc, 0xdd],
        };
        let registry = InMemorySplitterRegistry::builder()
            .register(Arc::new(StubSplitter { key: key.clone() }))
            .build();
        assert!(registry.resolve(&key).is_some());
    }

    #[test]
    fn registry_returns_none_for_unknown_key() {
        let registry = InMemorySplitterRegistry::builder().build();
        let key = CallMatchKey {
            chain_id: 1,
            to: addr("0x1111111111111111111111111111111111111111"),
            selector: [0x11; 4],
        };
        assert!(registry.resolve(&key).is_none());
    }

    #[test]
    #[should_panic(expected = "duplicate splitter match key")]
    fn registry_panics_on_duplicate_key() {
        let key = CallMatchKey {
            chain_id: 1,
            to: addr("0x1111111111111111111111111111111111111111"),
            selector: [0x22; 4],
        };
        let _ = InMemorySplitterRegistry::builder()
            .register(Arc::new(StubSplitter { key: key.clone() }))
            .register(Arc::new(StubSplitter { key }))
            .build();
    }

    #[test]
    fn registry_runs_split_with_context() {
        let key = CallMatchKey {
            chain_id: 1,
            to: addr("0x1111111111111111111111111111111111111111"),
            selector: [0x33; 4],
        };
        let registry = InMemorySplitterRegistry::builder()
            .register(Arc::new(StubSplitter { key: key.clone() }))
            .build();
        let splitter = registry.resolve(&key).unwrap();
        let from = addr("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let to = key.to.clone();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = SplitContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
        };
        let result = splitter.split(&ctx, &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to, to);
    }
}
