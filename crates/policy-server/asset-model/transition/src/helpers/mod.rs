//! Shared mutation primitives used by every reducer.
//! Reducers must never modify state fields directly; they go through these
//! helpers so that:
//!   - balance underflow / approval scope / position invariants are checked once
//!   - the `StateDelta` is populated automatically and consistently
//! Helpers operate on a *read-only* `&WalletState` plus an *in-progress*
//! `&mut StateDelta`. "Effective state" at any point is `state` overlaid
//! with every change accumulated in `delta` so far.

pub mod approval;
pub mod balance;
pub mod delta;
pub mod derived;
pub mod position;
