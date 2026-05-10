//! 디코더 공통 helper.
//!
//! schema_v260508의 `decoders/common.ts` (normalizeArgs, v3PathToHops 등)와
//! liam191의 `lookup_token_metadata`, `decode_hook_flags`,
//! `calculate_deadline_horizon` 패턴을 차용.

use serde_json::Value;

use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry;
use crate::types::{Address, AmountKind, AmountSpec, ChainId, RecipientFields, RecipientRef, Token};

// ===========================================================================
// args 추출
// ===========================================================================

/// args JSON 객체에서 키별 값을 강제 추출. 키 누락 시 `MissingArg`.
pub fn require_arg<'a>(args: &'a Value, name: &'static str) -> Result<&'a Value, SemiAdapterError> {
    args.get(name).ok_or(SemiAdapterError::MissingArg { name })
}

/// args의 string 값을 추출하고 uint256 십진 문자열로 가정.
pub fn as_uint_string(args: &Value, name: &'static str) -> Result<String, SemiAdapterError> {
    let v = require_arg(args, name)?;
    v.as_str()
        .map(String::from)
        .ok_or(SemiAdapterError::BadArgType {
            name,
            expected: "uint256 decimal string",
            got: type_name_of(v).into(),
        })
}

/// args의 string 값을 주소로 파싱.
pub fn as_address(args: &Value, name: &'static str) -> Result<Address, SemiAdapterError> {
    let v = require_arg(args, name)?;
    let s = v.as_str().ok_or(SemiAdapterError::BadArgType {
        name,
        expected: "address hex string",
        got: type_name_of(v).into(),
    })?;
    s.parse::<Address>()
        .map_err(|_| SemiAdapterError::BadAddress { value: s.into() })
}

/// args의 array 값을 주소 배열로 파싱.
pub fn as_address_array(args: &Value, name: &'static str) -> Result<Vec<Address>, SemiAdapterError> {
    let v = require_arg(args, name)?;
    let arr = v.as_array().ok_or(SemiAdapterError::BadArgType {
        name,
        expected: "address[]",
        got: type_name_of(v).into(),
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, e) in arr.iter().enumerate() {
        let s = e.as_str().ok_or(SemiAdapterError::BadArgType {
            name,
            expected: "address[]",
            got: format!("{}[{i}] is not string", type_name_of(e)),
        })?;
        out.push(
            s.parse::<Address>()
                .map_err(|_| SemiAdapterError::BadAddress { value: s.into() })?,
        );
    }
    Ok(out)
}

/// args의 number/string 값을 u64로 파싱 (deadline 등).
pub fn as_u64(args: &Value, name: &'static str) -> Result<u64, SemiAdapterError> {
    let v = require_arg(args, name)?;
    if let Some(n) = v.as_u64() {
        return Ok(n);
    }
    if let Some(s) = v.as_str() {
        return s.parse::<u64>().map_err(|_| SemiAdapterError::BadUintString {
            value: s.into(),
        });
    }
    Err(SemiAdapterError::BadArgType {
        name,
        expected: "u64 number or decimal string",
        got: type_name_of(v).into(),
    })
}

fn type_name_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// ===========================================================================
// V3 encoded path 분해 — `tokenIn(20B) + fee(3B) + token(20B) + fee(3B) + tokenOut(20B)`
// ===========================================================================

/// V3 encoded path bytes를 hop 시퀀스로 분해.
/// 길이 검증: `20 + (3 + 20) * k` (k ≥ 1).
///
/// `reverse = true`이면 path가 역순으로 인코딩됨 (V3 exactOutput 변형).
pub fn v3_path_to_hops(
    path: &[u8],
    chain_id: ChainId,
    reverse: bool,
) -> Result<Vec<V3Hop>, SemiAdapterError> {
    if path.len() < 20 + 3 + 20 {
        return Err(SemiAdapterError::BadV3Path { length: path.len() });
    }
    if !(path.len() - 20).is_multiple_of(23) {
        return Err(SemiAdapterError::BadV3Path { length: path.len() });
    }

    let hop_count = (path.len() - 20) / 23;
    let mut hops = Vec::with_capacity(hop_count);

    for i in 0..hop_count {
        let offset = i * 23;
        let token_a_bytes = &path[offset..offset + 20];
        let fee_bytes = &path[offset + 20..offset + 23];
        let token_b_bytes = &path[offset + 23..offset + 43];

        let token_a = Address::from_slice(token_a_bytes);
        let token_b = Address::from_slice(token_b_bytes);
        // 3-byte BE → u32
        let fee = ((fee_bytes[0] as u32) << 16) | ((fee_bytes[1] as u32) << 8) | (fee_bytes[2] as u32);

        let (in_addr, out_addr) = if reverse { (token_b, token_a) } else { (token_a, token_b) };
        hops.push(V3Hop {
            token_in: registry::token_metadata(in_addr, chain_id),
            token_out: registry::token_metadata(out_addr, chain_id),
            fee_tier: fee,
        });
    }
    Ok(hops)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V3Hop {
    pub token_in: Token,
    pub token_out: Token,
    /// `feeTier`. fee_bps = `fee_tier / 100`.
    pub fee_tier: u32,
}

// ===========================================================================
// AmountSpec / AmountKind 빌더
// ===========================================================================

pub fn amount_exact(raw: impl Into<String>) -> AmountSpec {
    AmountSpec {
        raw: raw.into(),
        kind: AmountKind::Exact,
    }
}

pub fn amount_min(raw: impl Into<String>) -> AmountSpec {
    AmountSpec {
        raw: raw.into(),
        kind: AmountKind::Min,
    }
}

pub fn amount_max(raw: impl Into<String>) -> AmountSpec {
    AmountSpec {
        raw: raw.into(),
        kind: AmountKind::Max,
    }
}

/// `type(uint256).max` 또는 `type(uint160).max` 패턴이면 `Unlimited`로 정규화.
pub fn amount_with_unlimited_check(raw: String) -> AmountSpec {
    const UINT256_MAX: &str =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    const UINT160_MAX: &str = "1461501637330902918203684832716283019655932542975";
    let kind = if raw == UINT256_MAX || raw == UINT160_MAX {
        AmountKind::Unlimited
    } else {
        AmountKind::Exact
    };
    AmountSpec { raw, kind }
}

// ===========================================================================
// Recipient / Deadline 도출
// ===========================================================================

/// recipient + actor → `RecipientFields` 구성. 동등성 boolean 자동 채움.
pub fn recipients_from(recipient: Option<Address>, actor: Address) -> RecipientFields {
    match recipient {
        Some(addr) if addr == actor => RecipientFields {
            recipient: Some(RecipientRef::Address { address: addr }),
            recipient_equals_actor: true,
            has_external_recipient: false,
        },
        Some(addr) => RecipientFields {
            recipient: Some(RecipientRef::Address { address: addr }),
            recipient_equals_actor: false,
            has_external_recipient: true,
        },
        None => RecipientFields {
            recipient: Some(RecipientRef::Actor),
            recipient_equals_actor: true,
            has_external_recipient: false,
        },
    }
}

/// liam191 `calculate_deadline_horizon` 차용. block_ts 알면 horizon 계산.
pub fn deadline_horizon(deadline: u64, block_ts: Option<u64>) -> Option<i64> {
    block_ts.map(|ts| deadline as i64 - ts as i64)
}

// ===========================================================================
// Hook flags (V4 hook 14bit 권한)
// ===========================================================================

/// V4 Hook 주소 마지막 14비트 → swap 관련 권한 추출.
///
/// Uniswap V4 IHooks Permissions:
///   bit 0 = beforeInitialize
///   bit 1 = afterInitialize
///   bit 2 = beforeAddLiquidity
///   bit 3 = afterAddLiquidity
///   bit 4 = beforeRemoveLiquidity
///   bit 5 = afterRemoveLiquidity
///   bit 6 = beforeSwap
///   bit 7 = afterSwap
///   bit 8 = beforeDonate
///   bit 9 = afterDonate
///   bit 10 = beforeSwapReturnsDelta
///   bit 11 = afterSwapReturnsDelta
///   bit 12 = afterAddLiquidityReturnsDelta
///   bit 13 = afterRemoveLiquidityReturnsDelta
///
/// liam191 차용 — swap 관련만 (bits 6, 7, 10, 11) 필터.
pub fn swap_hook_flags(hooks_addr: Address) -> Vec<&'static str> {
    if hooks_addr == Address::ZERO {
        return Vec::new();
    }
    let bytes = hooks_addr.as_slice();
    // 마지막 2바이트가 14비트 권한 (LSB 쪽)
    let low = ((bytes[18] as u16) << 8) | (bytes[19] as u16);
    let mut out = Vec::new();
    if low & (1 << 6) != 0 {
        out.push("beforeSwap");
    }
    if low & (1 << 7) != 0 {
        out.push("afterSwap");
    }
    if low & (1 << 10) != 0 {
        out.push("beforeSwapReturnsDelta");
    }
    if low & (1 << 11) != 0 {
        out.push("afterSwapReturnsDelta");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn require_arg_missing() {
        let args = json!({"foo": 1});
        assert!(matches!(
            require_arg(&args, "bar"),
            Err(SemiAdapterError::MissingArg { name: "bar" })
        ));
    }

    #[test]
    fn as_uint_string_ok() {
        let args = json!({"amount": "1000000"});
        assert_eq!(as_uint_string(&args, "amount").unwrap(), "1000000");
    }

    #[test]
    fn v3_path_decode_2hop() {
        // USDC (20B) + 0x0001f4 (500) + WETH (20B) — 단일 hop
        let mut path = Vec::new();
        path.extend_from_slice(&[0xa0, 0xb8, 0x69, 0x91, 0xc6, 0x21, 0x8b, 0x36, 0xc1, 0xd1, 0x9d, 0x4a, 0x2e, 0x9e, 0xb0, 0xce, 0x36, 0x06, 0xeb, 0x48]);
        path.extend_from_slice(&[0x00, 0x01, 0xf4]); // fee = 500
        path.extend_from_slice(&[0xc0, 0x2a, 0xaa, 0x39, 0xb2, 0x23, 0xfe, 0x8d, 0x0a, 0x0e, 0x5c, 0x4f, 0x27, 0xea, 0xd9, 0x08, 0x3c, 0x75, 0x6c, 0xc2]);
        let hops = v3_path_to_hops(&path, 1, false).unwrap();
        assert_eq!(hops.len(), 1);
        assert_eq!(hops[0].fee_tier, 500);
        assert_eq!(hops[0].token_in.symbol, "USDC");
        assert_eq!(hops[0].token_out.symbol, "WETH");
    }

    #[test]
    fn unlimited_detection_uint256_max() {
        let amount = amount_with_unlimited_check(
            "115792089237316195423570985008687907853269984665640564039457584007913129639935".into(),
        );
        assert_eq!(amount.kind, AmountKind::Unlimited);
    }

    #[test]
    fn unlimited_detection_uint160_max() {
        let amount = amount_with_unlimited_check(
            "1461501637330902918203684832716283019655932542975".into(),
        );
        assert_eq!(amount.kind, AmountKind::Unlimited);
    }

    #[test]
    fn recipients_self_vs_external() {
        let actor: Address = "0x1111111111111111111111111111111111111111".parse().unwrap();
        let other: Address = "0x2222222222222222222222222222222222222222".parse().unwrap();
        let r1 = recipients_from(Some(actor), actor);
        assert!(r1.recipient_equals_actor);
        assert!(!r1.has_external_recipient);
        let r2 = recipients_from(Some(other), actor);
        assert!(!r2.recipient_equals_actor);
        assert!(r2.has_external_recipient);
    }

    #[test]
    fn deadline_horizon_calc() {
        assert_eq!(deadline_horizon(1000, Some(900)), Some(100));
        assert_eq!(deadline_horizon(1000, None), None);
    }
}
