//! Rust struct ↔ SQL row 변환.
//!
//! Phase 1 의 대상:
//! * [`token_key`] — `TokenKey` enum → 16-byte hash + 평탄 컬럼 (standard / chain
//!   / address / contract / `token_id`)
//! * [`balance`] — `Balance` enum → (`form_tag`, `amount_decimal_string`)
//! * [`live_field`] — `LiveField<Price>` → 평탄 컬럼 (`value/synced_at/ttl/conf`)
//!   + `DataSource` JSON

pub mod balance;
pub mod live_field;
pub mod token_key;

pub use balance::{decode_balance, encode_balance, BalanceColumns};
pub use live_field::{decode_price_live_field, encode_price_live_field, LiveFieldColumns};
pub use token_key::{decode_token_key, encode_token_key, token_hash, TokenColumns};
