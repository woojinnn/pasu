//! Generic ABI 디코더 — `alloy-dyn-abi` 기반.
//!
//! 새 protocol 의 view 함수 추가 = `AbiTypeRegistry` 에 ABI 시그니처 한 줄 등록.
//!
//! 사용 예:
//! ```ignore
//! let mut reg = AbiTypeRegistry::with_builtins();
//! reg.register("my_view_fn", "(uint256,address)")?;
//!
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

/// generic ABI 디코더 — `decoder_id` 기반 dispatch + struct shape 매핑.
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

    /// `decoder_id` 가 known ABI 시그니처인지.
    #[must_use]
    pub fn knows(&self, decoder_id: &str) -> bool {
        self.types.get(decoder_id).is_some()
    }

    /// raw returndata 를 디코드해서 JSON Value 로.
    ///
    /// 흐름:
    ///   1. `types` 에서 ABI 시그니처 찾아 generic decode
    ///   2. `mappers` 에 매퍼 등록돼있으면 array → struct shape 변환 적용
    ///
    /// 결과: 매퍼 있으면 typed struct JSON object, 없으면 raw array.
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

        // 매퍼 적용 (있으면 typed struct shape 으로, 없으면 raw 그대로)
        Ok(self.mappers.maybe_apply(decoder_id, raw))
    }

    /// `decoder_id` 의 ABI 시그니처 (debugging 용).
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
        // returns (uint256) 의 32-byte big-endian "100"
        let mut data = [0u8; 32];
        data[31] = 100;
        let json = decoder.decode("abi_u256", &data).unwrap();
        assert_eq!(json, Value::String("100".into()));
    }

    #[test]
    fn decodes_aave_v3_user_account_data() {
        // 6 × uint256 = 192 byte. 각 32-byte 슬롯에 1, 2, 3, 4, 5, 6.
        // 이제 mapper 가 적용돼서 UserLendingState shape JSON object 가 결과.
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
        // health_factor = 6 (ray) → "0.000000000000000000000000006" 로 scale
        // (테스트는 매우 작은 ray 라 그렇지만 변환 로직 동작 확인)
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

    /// 가짜 V3 slot0 데이터 — 실제 슬롯 7 필드.
    #[test]
    fn decodes_uniswap_v3_slot0_shape() {
        let decoder = AbiDecoder::default();
        // 7 슬롯 = uint160 + int24 + 3×uint16 + uint8 + bool
        // alloy 가 모두 32-byte slot 으로 패딩. 총 224 byte
        let mut data = vec![0u8; 32 * 7];
        // sqrtPriceX96 = 4 (단순 placeholder)
        data[31] = 4;
        let json = decoder.decode("uniswap_v3_slot0", &data).unwrap();
        let arr = json.as_array().expect("expected array");
        assert_eq!(arr.len(), 7);
        assert_eq!(arr[0], Value::String("4".into())); // sqrtPriceX96
    }
}
