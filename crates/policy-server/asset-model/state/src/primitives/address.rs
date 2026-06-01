//! 주소 타입. alloy-primitives 의 `Address` 를 그대로 re-export.
//!
//! Spec 컨벤션: 항상 lowercase. alloy `Address` 는 EIP-55 checksum 도 표현 가능하지만,
//! 비교/저장 시에는 lowercase hex 로 정규화한다.
//!
//! `tsify-next` 는 외부 crate 의 `alloy_primitives::Address` 에 `Tsify` 를 자동
//! derive 하지 못한다. 모든 사용처에서는 `#[tsify(type = "string")]` 으로 TS
//! 타입을 강제하거나, 컨테이너 type alias 가 string 임을 명시한다.

pub use alloy_primitives::Address;

/// "0x...40hex" lowercase 문자열로 정규화한다.
#[must_use]
pub fn lowercase_hex(addr: &Address) -> String {
    format!("{addr:#x}")
}

/// spender 는 의미상 Address 와 같지만, approval 컨텍스트에서 명시적으로 구분.
pub type Spender = Address;
