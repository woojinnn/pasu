//! ```ignore
//! let mut reg = AbiTypeRegistry::with_builtins();
//! reg.register("my_view_fn", "(uint256,address)")?;
//! let decoder = AbiDecoder::new(reg);
//! let json = decoder.decode("aave_v3_user_account_data", &returndata)?;
//! ```

pub mod mappers;
pub mod sourcify;
pub mod types;
pub mod value_to_json;

pub use mappers::{MapperFn, MapperRegistry};
pub use types::AbiTypeRegistry;

use alloy_dyn_abi::DynSolType;
use serde_json::Value;

use crate::error::SyncError;

#[derive(Debug)]
pub struct AbiDecoder {
    types: AbiTypeRegistry,
    mappers: MapperRegistry,
}

impl Default for AbiDecoder {
    fn default() -> Self {
        Self::new(
            AbiTypeRegistry::with_builtins(),
            MapperRegistry::with_builtins(),
        )
    }
}

impl AbiDecoder {
    #[must_use]
    pub const fn new(types: AbiTypeRegistry, mappers: MapperRegistry) -> Self {
        Self { types, mappers }
    }

    pub const fn types_mut(&mut self) -> &mut AbiTypeRegistry {
        &mut self.types
    }

    pub const fn mappers_mut(&mut self) -> &mut MapperRegistry {
        &mut self.mappers
    }

    #[must_use]
    pub fn knows(&self, decoder_id: &str) -> bool {
        self.types.get(decoder_id).is_some()
    }

    pub fn decode(&self, decoder_id: &str, data: &[u8]) -> Result<Value, SyncError> {
        let ty = self
            .types
            .get(decoder_id)
            .ok_or_else(|| SyncError::UnknownDecoder(format!("abi_decoder: {decoder_id}")))?;

        let decoded = ty.abi_decode(data).map_err(|e| SyncError::FetchFailed {
            source_id: "abi_decoder".into(),
            reason: format!("abi decode '{decoder_id}': {e}"),
        })?;

        let raw = if let alloy_dyn_abi::DynSolValue::Tuple(items) = &decoded {
            value_to_json::flatten_function_result(items)
        } else {
            value_to_json::dyn_to_json(&decoded)
        };

        Ok(self.mappers.maybe_apply(decoder_id, raw))
    }

    #[must_use]
    pub fn signature_of(&self, decoder_id: &str) -> Option<&DynSolType> {
        self.types.get(decoder_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_single_uint256() {
        let decoder = AbiDecoder::default();
        let mut data = [0u8; 32];
        data[31] = 100;
        let json = decoder.decode("abi_u256", &data).unwrap();
        assert_eq!(json, Value::String("100".into()));
    }

    #[test]
    fn decodes_aave_v3_user_account_data() {
        let decoder = AbiDecoder::default();
        let mut data = vec![0u8; 192];
        for i in 0..6 {
            data[(i + 1) * 32 - 1] = (i + 1) as u8;
        }
        let json = decoder.decode("aave_v3_user_account_data", &data).unwrap();
        let obj = json.as_object().expect("mapper should produce object");
        assert_eq!(obj["total_collat_usd"], Value::String("1".into()));
        assert_eq!(obj["total_debt_usd"], Value::String("2".into()));
        assert_eq!(obj["available_borrow_usd"], Value::String("3".into()));
        assert!(obj.contains_key("health_factor"));
    }

    #[test]
    fn unknown_id_errors() {
        let decoder = AbiDecoder::default();
        let err = decoder.decode("nonexistent", &[]).unwrap_err();
        assert!(matches!(err, SyncError::UnknownDecoder(_)));
    }

    #[test]
    fn knows_returns_correctly() {
        let decoder = AbiDecoder::default();
        assert!(decoder.knows("aave_v3_user_account_data"));
        assert!(decoder.knows("uniswap_v3_slot0"));
        assert!(!decoder.knows("nope"));
    }

    #[test]
    fn decodes_uniswap_v3_slot0_shape() {
        let decoder = AbiDecoder::default();
        let mut data = vec![0u8; 32 * 7];
        data[31] = 4;
        let json = decoder.decode("uniswap_v3_slot0", &data).unwrap();
        let arr = json.as_array().expect("expected array");
        assert_eq!(arr.len(), 7);
        assert_eq!(arr[0], Value::String("4".into())); // sqrtPriceX96
    }
}
