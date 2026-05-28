//! Token 관련 타입.

/// 한 fungibility 단위의 보유 상태 (`Balance`, `TokenHolding`).
pub mod holding;
/// fungibility 단위 식별자 (`TokenKey`, `TokenId`).
pub mod key;
/// 토큰의 의미 분류 (`TokenKind` + 보조 enum 10종).
pub mod kind;
/// LP share 의 모양 (`LpShape`, `RangeSpec`, `ShareForm`).
pub mod lp;
/// `TokenKind` 안에서 다른 토큰을 가리키는 가벼운 ref (`TokenRef`).
pub mod token_ref;

pub use holding::{Balance, TokenHolding};
pub use key::{TokenId, TokenKey};
pub use kind::{
    BaseCategory, FiatCurrency, NoteKind, PegKind, PegTarget, RateMode, RebaseForm, TokenKind,
    UnlockSchedule,
};
pub use lp::{LpShape, RangeSpec, ShareForm};
pub use token_ref::TokenRef;
