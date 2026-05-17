//! Multi-call splitter — turns one transaction calldata into N sub-calls.
//!
//! # Why
//!
//! Most Ethereum transactions are a single contract call: one selector,
//! one ABI decode, one mapper invocation. Some routers (Uniswap Universal
//! Router, Safe `multiSend`, Multicall3, …) pack multiple sub-operations
//! into a single outer `execute(...)` calldata. The downstream pipeline
//! (resolver → mapper → compactor) is built to process one
//! [`SubCall`] at a time, so the splitter is the first thing that runs:
//! it normalises every transaction into a list of sub-calls (length 1 for
//! plain calls, N for multi-call routers).
//!
//! # Status
//!
//! This module is the **scaffold** for the splitter pipeline. It defines
//! [`SubCall`], [`SplitContext`], [`Splitter`], [`SplitError`], and an
//! [`IdentitySplitter`] that wraps the input as a single sub-call. Concrete
//! protocol splitters (Universal Router, Safe multiSend) land in later
//! phases and reuse the [`subdecode::opcode_stream`](crate::subdecode::opcode_stream)
//! dispatcher for their opcode-level decoding.

mod identity;
mod registry;

use policy_engine::action::{Address, DecimalString};

pub use identity::IdentitySplitter;
pub use registry::{InMemorySplitterRegistry, InMemorySplitterRegistryBuilder, SplitterRegistry};

use crate::CallMatchKey;

/// One sub-call produced by a [`Splitter`]. Carries everything a downstream
/// resolver/mapper needs to treat the sub-call as if it were a standalone
/// top-level transaction call.
///
/// `calldata` includes the 4-byte selector prefix so the resolver can index
/// by `(chain_id, to, selector)` the same way it would for a top-level call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubCall {
    /// Destination contract for the sub-call. For a UR-wrapped V3 swap this
    /// is the V3 SwapRouter; for a direct call it's the original `tx.to`.
    pub to: Address,
    /// `msg.value` attributable to this sub-call. Splitters that can't
    /// determine the per-sub-call value (most multi-call routers) leave it
    /// at zero — the surrounding context already carries `value_wei`.
    pub value_wei: DecimalString,
    /// Full calldata (4-byte selector + ABI args). Empty `Vec` is allowed
    /// for value-only transfers, but most sub-calls will carry a selector.
    pub calldata: Vec<u8>,
}

/// Context handed to a [`Splitter`] when it walks an outer transaction.
///
/// All fields are borrowed from the surrounding `RouterContext` /
/// `CallContext`. Splitters generally don't need the full registry stack
/// because they emit `SubCall`s for downstream stages to resolve.
pub struct SplitContext<'a> {
    pub chain_id: u64,
    pub from: &'a Address,
    pub to: &'a Address,
    pub value_wei: &'a DecimalString,
    pub block_timestamp: Option<u64>,
}

/// A splitter knows how to recognise and unpack one multi-call format.
/// Multiple splitter instances are registered with [`SplitterRegistry`] and
/// dispatched by `(chain_id, to, selector)` exactly like [`crate::CallMatchKey`].
pub trait Splitter: Send + Sync {
    /// Match keys this splitter responds to. The registry indexes splitters
    /// by these tuples; a top-level call whose key doesn't match any
    /// registered splitter falls through to the [`IdentitySplitter`].
    fn match_keys(&self) -> Vec<CallMatchKey>;

    /// Split `calldata` into one or more sub-calls. Returning a single-
    /// element vec is allowed (e.g. when the outer call only wraps one inner
    /// operation), in which case the result is observationally equivalent to
    /// the identity splitter.
    fn split(&self, ctx: &SplitContext<'_>, calldata: &[u8]) -> Result<Vec<SubCall>, SplitError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SplitError {
    #[error("calldata shorter than 4-byte selector ({0} bytes)")]
    CalldataTooShort(usize),
    #[error("outer ABI decode failed: {0}")]
    OuterDecode(String),
    #[error("opcode stream produced an unknown step: opcode=0x{0:02x}")]
    UnknownOpcode(u8),
    #[error("opcode {opcode_name} carried no decoded args (input did not match its ABI)")]
    MissingArgs { opcode_name: &'static str },
    #[error("internal: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr as _;

    fn addr(s: &str) -> Address {
        Address::from_str(s).unwrap()
    }
    fn dec(s: &str) -> DecimalString {
        DecimalString::from_str(s).unwrap()
    }

    #[test]
    fn sub_call_round_trip_equality() {
        let a = SubCall {
            to: addr("0x1111111111111111111111111111111111111111"),
            value_wei: dec("0"),
            calldata: vec![0x09, 0x5e, 0xa7, 0xb3, 0x00],
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
