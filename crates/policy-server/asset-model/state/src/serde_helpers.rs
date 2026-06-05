//! JSON serde helpers.
//!
//! JSON object keys must be strings, so `BTreeMap<K, V>` values whose keys are
//! enums or tuples cannot be represented directly. This module serializes those
//! maps as `Vec<(K, V)>` pairs instead.
//!
//! Example:
//! ```ignore
//! #[serde(with = "crate::serde_helpers::map_as_pairs")]
//! pub tokens: BTreeMap<TokenKey, TokenHolding>,
//! ```

/// Serializes and deserializes a `BTreeMap<K, V>` as JSON pairs.
pub mod map_as_pairs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    /// Serializes the map as a sequence of `(key, value)` pairs.
    ///
    /// # Errors
    ///
    /// Returns the serializer's error if any key or value cannot be serialized.
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
    ///
    /// # Errors
    ///
    /// Returns the deserializer's error if the sequence or one of its entries is invalid.
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
