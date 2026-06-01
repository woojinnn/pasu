//! `DynSolValue` → `serde_json::Value` 변환.
//!
//! alloy-dyn-abi 가 디코드한 결과를 sync 가 쓰는 JSON 형태로 평탄화.
//! `Uint(256, "100")` 같은 typed value 를 `"100"` (decimal string) 으로.
//! Tuple/Array 는 JSON Array, Address 는 hex string 등.

use alloy_dyn_abi::DynSolValue;
use serde_json::Value;

/// 한 `DynSolValue` 를 `serde_json::Value` 로.
pub fn dyn_to_json(v: &DynSolValue) -> Value {
    match v {
        DynSolValue::Bool(b) => Value::Bool(*b),

        DynSolValue::Int(i, _bits) => {
            // 음수도 가능 — decimal string 으로
            Value::String(i.to_string())
        }
        DynSolValue::Uint(u, _bits) => Value::String(u.to_string()),

        DynSolValue::FixedBytes(bytes, _len) => {
            Value::String(format!("0x{}", hex::encode(bytes.as_slice())))
        }
        DynSolValue::Address(addr) => Value::String(format!("{addr:#x}")),
        DynSolValue::Function(f) => Value::String(format!("0x{}", hex::encode(f.as_slice()))),

        DynSolValue::Bytes(bytes) => Value::String(format!("0x{}", hex::encode(bytes))),
        DynSolValue::String(s) => Value::String(s.clone()),

        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => {
            Value::Array(items.iter().map(dyn_to_json).collect())
        }

        DynSolValue::Tuple(items) => {
            // Tuple 은 JSON Array 로 변환 (sync 의 디코더 convention 과 매칭)
            Value::Array(items.iter().map(dyn_to_json).collect())
        }
    }
}

/// Top-level tuple (= 함수의 returns) 을 평탄화.
///
/// 함수가 단일 반환값이면 `result[0]` 만 풀어 그 값 자체로 반환.
/// 다중 반환값이면 JSON Array 그대로.
pub fn flatten_function_result(values: &[DynSolValue]) -> Value {
    match values.len() {
        0 => Value::Null,
        1 => dyn_to_json(&values[0]),
        _ => Value::Array(values.iter().map(dyn_to_json).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, I256, U256};

    #[test]
    fn uint_to_decimal_string() {
        let v = DynSolValue::Uint(U256::from(42u64), 256);
        assert_eq!(dyn_to_json(&v), Value::String("42".into()));
    }

    #[test]
    fn int_negative_to_string() {
        let v = DynSolValue::Int(I256::try_from(-100i64).unwrap(), 256);
        // I256 의 to_string 은 음수 표기 포함
        let json = dyn_to_json(&v);
        assert!(json.as_str().unwrap().starts_with('-'));
    }

    #[test]
    fn address_to_hex() {
        let v = DynSolValue::Address(Address::ZERO);
        assert_eq!(
            dyn_to_json(&v),
            Value::String("0x0000000000000000000000000000000000000000".into()),
        );
    }

    #[test]
    fn tuple_to_array() {
        let v = DynSolValue::Tuple(vec![
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Uint(U256::from(2u64), 256),
        ]);
        let json = dyn_to_json(&v);
        assert_eq!(
            json,
            Value::Array(vec![Value::String("1".into()), Value::String("2".into())])
        );
    }

    #[test]
    fn bool_passthrough() {
        let v = DynSolValue::Bool(true);
        assert_eq!(dyn_to_json(&v), Value::Bool(true));
    }
}
