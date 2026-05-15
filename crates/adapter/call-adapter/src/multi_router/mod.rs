//! Multi-router CallAdapter — for calls whose single `calldata` contains
//! multiple sub-calls (one outer ABI envelope wrapping N inner operations).
//!
//! The Mapper trait shape (`DecodedCall → ActionEnvelope[]`) can't express
//! this cleanly because the inner sub-calls need their own ABI decode against
//! per-sub-call schemas — that's `Decoder` territory, not `Mapper`. So we
//! drop down to the `CallAdapter` trait, which receives raw calldata and can
//! do whatever internal decoding it needs.
//!
//! # Fork registration
//!
//! Universal-Router-style routers are widely forked (Pancake, …). Each fork
//! shares the outer `execute(commands, inputs[, deadline])` ABI shape but
//! uses its own opcode table and is deployed at its own addresses. Rather
//! than copying the dispatcher per fork, [`MultiRouterCallAdapter`] takes a
//! `(deployments, opcode_constants)` tuple and constructs an adapter
//! instance. Add a fork by calling [`MultiRouterCallAdapter::new`] (or one
//! of the factory methods like [`MultiRouterCallAdapter::uniswap_ur`])
//! and registering it with `CallAdapterRegistry`.
//!
//! Currently registered factories:
//!   - [`MultiRouterCallAdapter::uniswap_ur`] — Uniswap UR mainnet/L2
//!     deployments, `UNISWAP_UR` opcode table.
//!
//! Future candidates that fit the same "1 calldata → N sub-calls" pattern:
//! - Pancake Universal Router (different opcode table — PR 6b)
//! - Safe `multiSend(bytes transactions)` (packed sub-tx list — needs a
//!   different outer decode and is better as a sibling adapter)
//! - 1inch aggregator multicall
//!
//! Module layout:
//!   - `execute` — outer ABI decode for the two `execute(...)` overloads
//!   - `commands` — opcode-stream dispatcher + `OpcodeConstants` plug point
//!   - `command_decode/` — per-opcode inner-input decoders
//!   - `v4_actions/` — V4Router inner-action decoders (dispatched from V4_SWAP)
//!   - `merge` — collapse Wrap+Swap / Swap+Unwrap plumbing pairs
//!   - `common` — shared word readers, asset/recipient helpers, V3 path parser

mod command_decode;
pub mod commands;
mod common;
mod execute;
mod sim;
mod v4_actions;

use abi_resolver::subdecode::protocols::pancake_ur::pancake_universal_router_deployments;
use abi_resolver::subdecode::protocols::universal_router::{
    uniswap_universal_router_deployments, EXECUTE_DEADLINE_SELECTOR, EXECUTE_SELECTOR,
};
use abi_resolver::CallMatchKey;
use policy_engine::action::{ActionEnvelope, Address};

use crate::{AdapterError, CallAdapter, CallAdapterId, CallContext};

use commands::OpcodeConstants;

/// One fork of a Universal-Router-style multi-call router. The dispatcher
/// is shared; the fork-specific bits are the deployment allowlist and the
/// opcode mapping.
#[derive(Debug, Clone)]
pub struct MultiRouterCallAdapter {
    id: CallAdapterId,
    deployments: Vec<(u64, Address)>,
    opcode_constants: OpcodeConstants,
}

impl MultiRouterCallAdapter {
    /// Generic constructor — wire a fresh fork into the dispatcher.
    ///
    /// `id` becomes the adapter's `CallAdapterId` (used for logging /
    /// registry de-dup). `deployments` are the `(chain_id, router_address)`
    /// pairs the fork is deployed at; calls to other addresses are not
    /// matched by this adapter. `opcode_constants` plugs into the dispatch
    /// loop in `commands::expand_commands`.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        deployments: Vec<(u64, Address)>,
        opcode_constants: OpcodeConstants,
    ) -> Self {
        Self {
            id: CallAdapterId::new(id),
            deployments,
            opcode_constants,
        }
    }

    /// Pre-built adapter for Uniswap Universal Router. Picks up the
    /// deployment allowlist from `abi_resolver::subdecode::protocols::
    /// universal_router::uniswap_universal_router_deployments()`.
    #[must_use]
    pub fn uniswap_ur() -> Self {
        let deployments = uniswap_universal_router_deployments()
            .map(|(chain_id, alloy_addr)| {
                (chain_id, common::policy_address_from_alloy(&alloy_addr))
            })
            .collect();
        Self::new(
            "multi-router/uniswap-universal-router",
            deployments,
            OpcodeConstants::UNISWAP_UR,
        )
    }

    /// Pre-built adapter for PancakeSwap (Infinity) Universal Router. Reuses
    /// the same dispatcher with a different opcode mapping (mask `0x3f`,
    /// see [`OpcodeConstants::PANCAKE_UR`]). Pancake Infinity sub-actions
    /// (opcodes 0x10–0x16) and stable-swap (0x22/0x23) are recognised but
    /// not yet decoded — they're treated as `Ignored` until dedicated
    /// decoders land. Common-range opcodes (V2/V3 swaps, WRAP/UNWRAP,
    /// Permit2 family, settlement utilities) work identically to Uniswap.
    #[must_use]
    pub fn pancake_ur() -> Self {
        let deployments = pancake_universal_router_deployments()
            .map(|(chain_id, alloy_addr)| {
                (chain_id, common::policy_address_from_alloy(&alloy_addr))
            })
            .collect();
        Self::new(
            "multi-router/pancake-universal-router",
            deployments,
            OpcodeConstants::PANCAKE_UR,
        )
    }
}

impl CallAdapter for MultiRouterCallAdapter {
    fn id(&self) -> CallAdapterId {
        self.id.clone()
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        let mut out = Vec::new();
        for (chain_id, to) in &self.deployments {
            for selector in [EXECUTE_SELECTOR, EXECUTE_DEADLINE_SELECTOR] {
                out.push(CallMatchKey {
                    chain_id: *chain_id,
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
        let envelopes = commands::expand_commands(
            ctx,
            &commands,
            &inputs,
            validity,
            0,
            &self.opcode_constants,
        )?;
        Ok(sim::simulate(envelopes, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::MultiRouterCallAdapter;
    use crate::CallAdapter as _;

    #[test]
    fn test_uniswap_ur_factory_match_keys_non_empty() {
        assert!(!MultiRouterCallAdapter::uniswap_ur().match_keys().is_empty());
    }

    #[test]
    fn test_pancake_ur_factory_match_keys_non_empty() {
        assert!(!MultiRouterCallAdapter::pancake_ur().match_keys().is_empty());
    }

    #[test]
    fn test_uniswap_and_pancake_have_distinct_ids() {
        assert_ne!(
            MultiRouterCallAdapter::uniswap_ur().id(),
            MultiRouterCallAdapter::pancake_ur().id(),
        );
    }
}
