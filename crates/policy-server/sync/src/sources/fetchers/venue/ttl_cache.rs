//! Tiny TTL cache: stores `(value, inserted_at)` per key and treats an entry as
//! a miss once it is older than `ttl`. Time is injected (no wall clock) so the
//! sync layer stays deterministic and testable.

use std::collections::HashMap;
use std::hash::Hash;

use policy_state::primitives::Time;

#[derive(Debug, Default)]
pub struct TtlCache<K, V> {
    entries: HashMap<K, (V, Time)>,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Returns the cached value if present and `now - inserted_at < ttl_secs`.
    #[must_use]
    pub fn get(&self, key: &K, now: Time, ttl_secs: u64) -> Option<V> {
        let (v, at) = self.entries.get(key)?;
        if now.as_unix().saturating_sub(at.as_unix()) < ttl_secs {
            Some(v.clone())
        } else {
            None
        }
    }

    pub fn put(&mut self, key: K, value: V, now: Time) {
        self.entries.insert(key, (value, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_within_ttl_then_miss_after_expiry() {
        let mut c: TtlCache<String, u32> = TtlCache::new();
        c.put("meta".into(), 7, Time::from_unix(1000));
        // within ttl=600 → hit
        assert_eq!(c.get(&"meta".into(), Time::from_unix(1599), 600), Some(7));
        // at/after ttl → miss
        assert_eq!(c.get(&"meta".into(), Time::from_unix(1600), 600), None);
        // unknown key → miss
        assert_eq!(c.get(&"other".into(), Time::from_unix(1000), 600), None);
    }
}
