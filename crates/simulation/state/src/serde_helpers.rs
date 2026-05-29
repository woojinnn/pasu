//! JSON serde helpers.
//!
//! Because JSON object keys must be strings, a `BTreeMap<K, V>` whose key is an
//! enum or tuple cannot be serialized directly. This module provides helpers
//! that serialize/deserialize such maps as a `Vec<(K, V)>` instead.
//!
//! Usage:
//! ```ignore
//! #[serde(with = "crate::serde_helpers::map_as_pairs")]
//! pub tokens: BTreeMap<TokenKey, TokenHolding>,
//! ```

/// Serde adapter that (de)serializes a `BTreeMap<K, V>` as a `Vec<(K, V)>`,
/// allowing maps with non-string keys to round-trip through JSON.
pub mod map_as_pairs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    /// Serializes the map as a sequence of `(key, value)` pairs.
    pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, ser: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Ord,
        V: Serialize,
        S: Serializer,
    {
        let vec: Vec<(&K, &V)> = map.iter().collect();
        vec.serialize(ser)
    }

    /// Deserializes a sequence of `(key, value)` pairs back into a `BTreeMap`.
    pub fn deserialize<'de, K, V, D>(de: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let vec: Vec<(K, V)> = Vec::deserialize(de)?;
        Ok(vec.into_iter().collect())
    }
}
