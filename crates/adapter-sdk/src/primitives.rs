//! Minimal numeric/byte primitives crossing the host/adapter boundary.
//! Addresses, hashes, selectors are serde-friendly hex strings on the wire.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address(pub [u8; 20]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct B256(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Selector(pub [u8; 4]);

pub type ChainId = u64;

#[derive(Debug, thiserror::Error)]
pub enum HexError {
    #[error("invalid hex string: {0}")]
    Decode(#[from] hex::FromHexError),
    #[error("wrong length: expected {expected}, got {got}")]
    Length { expected: usize, got: usize },
    #[error("missing 0x prefix")]
    MissingPrefix,
}

fn parse_fixed_hex<const N: usize>(s: &str) -> Result<[u8; N], HexError> {
    let stripped = s.strip_prefix("0x").ok_or(HexError::MissingPrefix)?;
    let bytes = hex::decode(stripped)?;
    if bytes.len() != N {
        return Err(HexError::Length { expected: N, got: bytes.len() });
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

macro_rules! hex_string_type {
    ($t:ident, $n:expr) => {
        impl FromStr for $t {
            type Err = HexError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok($t(parse_fixed_hex::<$n>(&s.to_ascii_lowercase())?))
            }
        }
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "0x{}", hex::encode(self.0))
            }
        }
        impl Serialize for $t {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }
        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                $t::from_str(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

hex_string_type!(Address, 20);
hex_string_type!(B256, 32);
hex_string_type!(Selector, 4);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_roundtrip_via_hex() {
        let a = Address::from_str("0xab5801a7d398351b8be11c439e05c5b3259aec9b").unwrap();
        assert_eq!(a.to_string(), "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
    }

    #[test]
    fn address_rejects_short() {
        let err = Address::from_str("0xab").unwrap_err();
        assert!(matches!(err, HexError::Length { .. }));
    }

    #[test]
    fn address_serde_json_lowercases() {
        let a = Address::from_str("0xAB5801a7D398351b8bE11C439e05C5B3259aeC9B").unwrap();
        let s = serde_json::to_string(&a).unwrap();
        assert_eq!(s, "\"0xab5801a7d398351b8be11c439e05c5b3259aec9b\"");
    }

    #[test]
    fn selector_from_hex() {
        let s = Selector::from_str("0xa9059cbb").unwrap();
        assert_eq!(s.0, [0xa9, 0x05, 0x9c, 0xbb]);
    }
}
