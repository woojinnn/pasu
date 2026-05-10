//! `SemiAdapterError` — decoder/dispatch 실패에 대한 상세 enum.
//!
//! liam191/scopeball-test의 `DecodeError` 패턴 차용 — 모든 variant가 *구체적
//! context*(need/got, expected/got 등)를 동반해 디버깅을 쉽게 함.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemiAdapterError {
    /// calldata 또는 args가 너무 짧아 디코딩 불가.
    #[error("입력 너무 짧음: 필요 {need}, 실제 {got}")]
    TooShort { need: usize, got: usize },

    /// 셀렉터가 예상과 다름.
    #[error("셀렉터 불일치: 예상 {expected}, 실제 {got}")]
    BadSelector { expected: String, got: String },

    /// ABI 디코딩 자체가 실패 (잘못된 타입, 길이 부족 등).
    #[error("ABI 디코드 실패: {reason}")]
    AbiDecode { reason: String },

    /// args JSON에 필요한 키가 없음.
    #[error("필수 인자 누락: {name}")]
    MissingArg { name: &'static str },

    /// args JSON 키의 타입이 예상과 다름.
    #[error("인자 {name}의 타입 잘못됨: 예상 {expected}, 실제 {got}")]
    BadArgType { name: &'static str, expected: &'static str, got: String },

    /// Universal Router opcode가 알려지지 않음.
    #[error("알 수 없는 opcode: 0x{opcode:02x} (family: {family})")]
    UnknownOpcode { opcode: u8, family: &'static str },

    /// Dispatch 룩업 실패 (selector / opcode / primaryType 모두 매칭 없음).
    #[error("dispatch 매칭 실패: {key}")]
    DispatchMiss { key: String },

    /// 토큰이 큐레이트 레지스트리에 없고 fallback도 사용 안 됨.
    #[error("토큰을 레지스트리에서 찾지 못함: {address}")]
    UnknownToken { address: String },

    /// V3 encoded path 분해 실패 (길이가 20 + 23k 형식 아님).
    #[error("V3 path 분해 실패: 길이 {length}는 20 + 23k 형식 아님")]
    BadV3Path { length: usize },

    /// uint256 십진 문자열 파싱 실패.
    #[error("uint256 파싱 실패: {value}")]
    BadUintString { value: String },

    /// 주소 hex 파싱 실패.
    #[error("주소 파싱 실패: {value}")]
    BadAddress { value: String },

    /// hex 문자열 파싱 실패.
    #[error("hex 파싱 실패: {0}")]
    BadHex(String),
}

impl SemiAdapterError {
    /// 다른 디코더로 fallback 가능한 *비치명적* 에러인지.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            SemiAdapterError::UnknownToken { .. }
                | SemiAdapterError::DispatchMiss { .. }
                | SemiAdapterError::UnknownOpcode { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_have_context() {
        let e = SemiAdapterError::TooShort { need: 132, got: 5 };
        assert!(e.to_string().contains("132"));
        assert!(e.to_string().contains("5"));
    }

    #[test]
    fn recoverable_classification() {
        assert!(SemiAdapterError::UnknownToken {
            address: "0x0".into()
        }
        .is_recoverable());
        assert!(!SemiAdapterError::TooShort { need: 4, got: 0 }.is_recoverable());
    }
}
