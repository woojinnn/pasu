//! Real-time event subsystem.
//!
//! Three pieces:
//! - [`types`]: typed `Event` enum + payloads that flow through the system.
//! - [`bus`]: in-process broadcast channel ([`EventBus`]) tagged by user_id.
//! - [`sse`]: `GET /events/stream` axum handler that bridges the bus to a
//!   per-client SSE response.

pub mod bus;
pub mod sse;
pub mod types;

pub use bus::EventBus;
pub use sse::stream as sse_stream;
pub use types::{Event, PolicyViolation, TxConfirmed, TxRef, TxRefWithHash, WalletSync};
