//! Manifest schema (spec §8). Macro-generated code embeds this as a JSON
//! string in a WASM custom section named `adapter_manifest`. The CLI reads
//! it back for validation and publishing.

use crate::primitives::{Address, ChainId};
use serde::{Deserialize, Serialize};

pub const SDK_VERSION: u32 = 1;
pub const CUSTOM_SECTION_NAME: &str = "adapter_manifest";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub sdk_version: u32,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applies_to: Vec<AppliesTo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub factory_of: Vec<FactoryOf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_of: Vec<ProxyOf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Decoder,
    CallAdapter,
    SignAdapter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliesTo {
    pub chain: ChainId,
    pub address: Address,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactoryOf {
    pub chain: ChainId,
    pub factory: Address,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyOf {
    pub chain: ChainId,
    pub implementation: Address,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest must claim at least one of applies_to / factory_of / proxy_of")]
    NoMatchers,
    #[error("sdk_version {got} unsupported; expected {expected}")]
    SdkVersion { expected: u32, got: u32 },
    #[error("capabilities list is empty")]
    NoCapabilities,
}

impl Manifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.sdk_version != SDK_VERSION {
            return Err(ManifestError::SdkVersion {
                expected: SDK_VERSION,
                got: self.sdk_version,
            });
        }
        if self.capabilities.is_empty() {
            return Err(ManifestError::NoCapabilities);
        }
        if self.applies_to.is_empty() && self.factory_of.is_empty() && self.proxy_of.is_empty()
        {
            return Err(ManifestError::NoMatchers);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn sample() -> Manifest {
        Manifest {
            name: "erc20-transfer".into(),
            version: "0.1.0".into(),
            sdk_version: SDK_VERSION,
            description: "ERC-20 transfer canary".into(),
            author: None,
            homepage: None,
            capabilities: vec![Capability::Decoder, Capability::CallAdapter],
            applies_to: vec![AppliesTo {
                chain: 1,
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
                    .unwrap(),
            }],
            factory_of: vec![],
            proxy_of: vec![],
        }
    }

    #[test]
    fn validate_accepts_well_formed_manifest() {
        assert!(sample().validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_matchers() {
        let mut m = sample();
        m.applies_to.clear();
        assert!(matches!(m.validate(), Err(ManifestError::NoMatchers)));
    }

    #[test]
    fn validate_rejects_unsupported_sdk_version() {
        let mut m = sample();
        m.sdk_version = 99;
        assert!(matches!(m.validate(), Err(ManifestError::SdkVersion { .. })));
    }

    #[test]
    fn manifest_json_roundtrip() {
        let m = sample();
        let s = serde_json::to_string(&m).unwrap();
        let back: Manifest = serde_json::from_str(&s).unwrap();
        assert_eq!(m, back);
    }
}
