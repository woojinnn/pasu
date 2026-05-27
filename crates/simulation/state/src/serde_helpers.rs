//! JSON serde 헬퍼들.
//!
//! JSON object 의 key 는 string 이어야 하므로, key 가 enum 이나 tuple 인
//! `BTreeMap<K, V>` 는 그대로 직렬화할 수 없다. 이 모듈은 해당 map 들을
//! `Vec<(K, V)>` 형태로 직렬화/역직렬화하는 helper 들을 제공한다.
//!
//! 사용:
//! ```ignore
//! #[serde(with = "crate::serde_helpers::map_as_pairs")]
//! pub tokens: BTreeMap<TokenKey, TokenHolding>,
//! ```

pub mod map_as_pairs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, ser: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Ord,
        V: Serialize,
        S: Serializer,
    {
        let vec: Vec<(&K, &V)> = map.iter().collect();
        vec.serialize(ser)
    }

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
