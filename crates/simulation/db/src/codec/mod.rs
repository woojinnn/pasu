//! Rust struct ↔ SQL row 변환.
//!
//! 한 도메인 타입을 SQL 컬럼들로 풀어쓰는 코드. 예: `TokenKind` enum 의 variant
//! 를 `kind_tag` 컬럼 + `kind_data` JSON 컬럼 한 쌍으로 표현.

// 단계적 활성화:
// pub mod token_kind;    // TokenKind  ↔ (kind_tag, kind_data)
// pub mod balance;       // Balance    ↔ (balance_kind, balance_data)
// pub mod live_field;    // LiveField  ↔ inline 5 컬럼 (value, source, synced_at, ttl, conf)
// pub mod position;      // Position   ↔ family 별 row 모양
// pub mod pending;       // PendingTx  ↔ row + commitment 평탄화
// pub mod delta;         // StateDelta ↔ JSON
