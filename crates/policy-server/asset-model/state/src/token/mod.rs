//! Token-related state types.

/// Holding state for one fungibility unit (`Balance`, `TokenHolding`).
pub mod holding;
/// Fungibility-unit identifiers (`TokenKey`, `TokenId`).
pub mod key;
/// Token semantic classification (`TokenKind` and supporting enums).
pub mod kind;
/// LP share shapes (`LpShape`, `RangeSpec`, `ShareForm`).
pub mod lp;
/// Lightweight references to tokens inside `TokenKind`.
pub mod token_ref;

pub use holding::{Balance, TokenHolding, TokenMetadata};
pub use key::{TokenId, TokenKey};
pub use kind::{
    BaseCategory, FiatCurrency, NoteKind, PegKind, PegTarget, RateMode, RebaseForm, TokenKind,
    UnlockSchedule,
};
pub use lp::{LpShape, RangeSpec, ShareForm};
pub use token_ref::TokenRef;
