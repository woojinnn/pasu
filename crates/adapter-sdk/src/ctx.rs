//! Host-provided context passed to trait methods.

use crate::error::{CtxError, LogLevel};
use crate::primitives::{Address, ChainId, Selector};
use crate::types::DecodedCall;

pub type LogFn<'a> = dyn Fn(LogLevel, &str) + 'a;
pub type LookupAdapterFn<'a> =
    dyn Fn(ChainId, Address, &[u8]) -> Result<DecodedCall, CtxError> + 'a;

pub struct CallCtx<'a> {
    pub chain_id: ChainId,
    pub target: Address,
    pub selector: Selector,
    pub log: &'a LogFn<'a>,
    pub lookup_adapter: &'a LookupAdapterFn<'a>,
}

pub struct SignCtx<'a> {
    pub chain_id: ChainId,
    pub verifying_contract: Address,
    pub primary_type: String,
    pub log: &'a LogFn<'a>,
    pub lookup_adapter: &'a LookupAdapterFn<'a>,
}

impl<'a> CallCtx<'a> {
    /// Convenience constructor for in-process tests that don't need recursion.
    pub fn for_test(
        chain_id: ChainId,
        target: Address,
        selector: Selector,
        log: &'a LogFn<'a>,
        lookup_adapter: &'a LookupAdapterFn<'a>,
    ) -> Self {
        Self { chain_id, target, selector, log, lookup_adapter }
    }
}
