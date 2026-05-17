//! Decoder trait — calldata → DecodedCall.

use std::sync::Arc;

use alloy_primitives::{I256, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DecoderId(pub String);

impl DecoderId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Match key for Decoder lookup: (chain_id, to address, function selector).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallMatchKey {
    pub chain_id: u64,
    pub to: policy_engine::action::Address,
    pub selector: [u8; 4],
}

impl Serialize for CallMatchKey {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        let mut s = ser.serialize_struct("CallMatchKey", 3)?;
        s.serialize_field("chainId", &self.chain_id)?;
        s.serialize_field("to", &self.to)?;
        let selector_hex = format!("0x{}", hex::encode(self.selector));
        s.serialize_field("selector", &selector_hex)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for CallMatchKey {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            chain_id: u64,
            to: policy_engine::action::Address,
            selector: String,
        }

        let r = Raw::deserialize(de)?;
        let s = r
            .selector
            .strip_prefix("0x")
            .ok_or_else(|| serde::de::Error::custom("selector must start with 0x"))?;
        let bytes = hex::decode(s).map_err(|e| serde::de::Error::custom(e.to_string()))?;
        if bytes.len() != 4 {
            return Err(serde::de::Error::custom("selector must be 4 bytes"));
        }

        let mut sel = [0u8; 4];
        sel.copy_from_slice(&bytes);
        Ok(CallMatchKey {
            chain_id: r.chain_id,
            to: r.to,
            selector: sel,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedArg {
    pub name: String,
    pub abi_type: String,
    pub value: DecodedValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecodedValue {
    Address(policy_engine::action::Address),
    Uint(U256),
    Int(I256),
    Bool(bool),
    Bytes(Vec<u8>),
    String(String),
    Array(Vec<DecodedValue>),
    Tuple(Vec<DecodedValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedCall {
    pub decoder_id: DecoderId,
    pub function_signature: String,
    pub args: Vec<DecodedArg>,
    pub nested: Vec<DecodedCall>,
}

#[derive(Debug)]
pub struct DecodeContext<'a> {
    pub chain_id: u64,
    pub to: &'a policy_engine::action::Address,
    pub value: &'a policy_engine::action::DecimalString,
    pub block_timestamp: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("unsupported selector")]
    UnsupportedSelector,
    #[error("invalid calldata: {0}")]
    InvalidCalldata(String),
    #[error("abi mismatch: {0}")]
    AbiMismatch(String),
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

pub trait Decoder: Send + Sync {
    fn id(&self) -> DecoderId;
    fn match_keys(&self) -> Vec<CallMatchKey>;
    fn decode(&self, ctx: &DecodeContext<'_>, calldata: &[u8])
        -> Result<DecodedCall, DecoderError>;
}

pub trait DecoderRegistry: Send + Sync {
    fn resolve(&self, key: &CallMatchKey) -> Option<Arc<dyn Decoder>>;
    fn match_keys(&self) -> Vec<CallMatchKey>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_call_match_key_serde_roundtrip() {
        let key = CallMatchKey {
            chain_id: 1,
            to: policy_engine::action::Address::from_str(
                "0x1111111111111111111111111111111111111111",
            )
            .unwrap(),
            selector: [0x38, 0xed, 0x17, 0x39],
        };

        let json = serde_json::to_value(&key).unwrap();
        assert_eq!(json["chainId"], 1);
        assert_eq!(json["to"], "0x1111111111111111111111111111111111111111");
        assert_eq!(json["selector"], "0x38ed1739");

        let decoded: CallMatchKey = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_call_match_key_selector_must_be_8_hex_chars() {
        for selector in ["0x38ed17", "0x38ed173900"] {
            let json = serde_json::json!({
                "chainId": 1,
                "to": "0x1111111111111111111111111111111111111111",
                "selector": selector,
            });

            let err = serde_json::from_value::<CallMatchKey>(json).unwrap_err();
            assert!(err.to_string().contains("selector must be 4 bytes"));
        }
    }
}
