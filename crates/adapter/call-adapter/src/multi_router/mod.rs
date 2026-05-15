//! Multi-router CallAdapter — for calls whose single `calldata` contains
//! multiple sub-calls (one outer ABI envelope wrapping N inner operations).
//!
//! The Mapper trait shape (`DecodedCall → ActionEnvelope[]`) can't express
//! this cleanly because the inner sub-calls need their own ABI decode against
//! per-sub-call schemas — that's `Decoder` territory, not `Mapper`. So we
//! drop down to the `CallAdapter` trait, which receives raw calldata and can
//! do whatever internal decoding it needs.
//!
//! Current coverage: Uniswap Universal Router's `execute(commands, inputs[, deadline])`.
//! Future candidates that fit the same "1 calldata → N sub-calls" pattern:
//! - Pancake Universal Router (different opcode table)
//! - Safe `multiSend(bytes transactions)` (packed sub-tx list)
//! - 1inch aggregator multicall
//! - Permit2 batch
//!
//! Module layout:
//!   - `execute` — outer ABI decode for the two `execute(...)` overloads
//!   - `commands` — opcode-stream dispatcher
//!   - `command_decode/` — per-opcode inner-input decoders
//!   - `v4_actions/` — V4Router inner-action decoders (dispatched from V4_SWAP)
//!   - `common` — shared word readers, asset/recipient helpers, V3 path parser

mod command_decode;
mod commands;
mod common;
mod execute;
mod v4_actions;

use abi_resolver::subdecode::protocols::universal_router::{
    uniswap_universal_router_deployments, EXECUTE_DEADLINE_SELECTOR, EXECUTE_SELECTOR,
};
use abi_resolver::CallMatchKey;
use policy_engine::action::ActionEnvelope;

use crate::{AdapterError, CallAdapter, CallAdapterId, CallContext};

const ADAPTER_ID: &str = "multi-router/uniswap-universal-router";

#[derive(Debug, Clone, Copy, Default)]
pub struct MultiRouterCallAdapter;

impl MultiRouterCallAdapter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CallAdapter for MultiRouterCallAdapter {
    fn id(&self) -> CallAdapterId {
        CallAdapterId::new(ADAPTER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        let mut out = Vec::new();
        for (chain_id, alloy_addr) in uniswap_universal_router_deployments() {
            let to = common::policy_address_from_alloy(&alloy_addr);
            for selector in [EXECUTE_SELECTOR, EXECUTE_DEADLINE_SELECTOR] {
                out.push(CallMatchKey {
                    chain_id,
                    to: to.clone(),
                    selector,
                });
            }
        }
        out
    }

    fn build(
        &self,
        ctx: &CallContext<'_>,
        calldata: &[u8],
    ) -> Result<Vec<ActionEnvelope>, AdapterError> {
        let (commands, inputs, validity) = execute::decode_outer_call(calldata)?;
        commands::expand_commands(ctx, &commands, &inputs, validity)
    }
}

#[cfg(test)]
mod tests {
    use super::MultiRouterCallAdapter;
    use crate::CallAdapter as _;

    #[test]
    fn test_ur_call_adapter_match_keys() {
        assert!(!MultiRouterCallAdapter::new().match_keys().is_empty());
    }
}
