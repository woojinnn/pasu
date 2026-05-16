//! `adapter-sdk` — public SDK for policy-engine adapter authors.
//!
//! Adapters compile to standalone WASM modules. This crate defines the trait
//! contract (`Decoder`, `CallAdapter`, `SignAdapter`), the data types crossing
//! the host/adapter boundary (`DecodedCall`, `ActionEnvelope`, …), and ABI
//! plumbing helpers consumed by the `#[adapter]` proc-macro.

pub mod abi;
pub mod action;
pub mod ctx;
pub mod error;
pub mod manifest;
pub mod primitives;
pub mod sign;
pub mod traits;
pub mod types;

pub mod prelude {
    // Re-exports populated by Tasks 2–8 as submodules gain real types.
    // pub use crate::action::{Action, ActionEnvelope};
    // pub use crate::ctx::{CallCtx, SignCtx};
    // pub use crate::error::{AdapterError, CtxError, LogLevel};
    // pub use crate::primitives::{Address, B256, Selector};
    // pub use crate::sign::{SignPayload, SignRequest};
    // pub use crate::traits::{CallAdapter, Decoder, SignAdapter};
    // pub use crate::types::{DecodedArg, DecodedCall, DecodedValue};
}
