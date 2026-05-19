//! Universal-Router-family splitter — Uniswap UR + PancakeSwap UR fork.
//!
//! Both forks share the same outer `execute(commands, inputs[, deadline])`
//! ABI shape and the same opcode-stream concept; they differ in the opcode
//! *mapping* (Pancake adds Infinity actions in the 0x10–0x16 / 0x22–0x23
//! ranges). The [`UniversalRouterSplitter`] is parameterised by an opcode
//! table reference plus a per-fork deployment allowlist so a single splitter
//! type covers every UR-style router.
//!
//! # SubCall format (Phase 3, intentional)
//!
//! Each opcode step is emitted as a [`SubCall`] whose:
//!
//! - `to` — the outer router contract (`ctx.to`). UR opcodes don't surface a
//!   per-step target address; the corresponding downstream mapper picks the
//!   real target based on the synthetic decoder id (e.g. WRAP_ETH → WETH9).
//! - `value_wei` — copied from the outer call. Only WRAP_ETH and the
//!   native-bearing variants of V3/V2 swaps actually consume msg.value;
//!   downstream mappers gate on opcode anyway.
//! - `calldata` — `[opcode_byte, ...raw_inner_input_bytes]`. Not a standard
//!   ABI selector + args layout — the first byte is the UR opcode, the rest
//!   is the original `inputs[i]` blob that the opcode dispatcher would feed
//!   to the per-opcode decoder. Phase 4 introduces UR-specific mappers that
//!   consume this shape directly.
//!
//! V4_SWAP (opcode 0x10) is **not** recursively split here — the V4 inner
//! action stream is its own opcode dispatch and a future revision will
//! either nest splitters or have the V4 mapper handle the action bytes
//! internally. For Phase 3 it surfaces as a single SubCall just like any
//! other opcode.

use std::sync::Arc;

use alloy_dyn_abi::DynSolValue;
use alloy_sol_types::{sol, SolCall};
use policy_engine::action::Address;

use crate::decoder::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use crate::ids::{
    UR_SWEEP_DECODER_ID, UR_TRANSFER_DECODER_ID, UR_UNWRAP_WETH_DECODER_ID,
    UR_V2_SWAP_EXACT_IN_DECODER_ID, UR_V2_SWAP_EXACT_OUT_DECODER_ID,
    UR_V3_SWAP_EXACT_IN_DECODER_ID, UR_V3_SWAP_EXACT_OUT_DECODER_ID, UR_V4_SWAP_DECODER_ID,
    UR_WRAP_ETH_DECODER_ID,
};
use crate::subdecode::opcode_stream::{dispatch as dispatch_opcodes, DecodedStep, OpcodeTable};
use crate::subdecode::protocols::pancake_ur::{
    pancake_universal_router_deployments, PANCAKE_UR_TABLE,
};
use crate::subdecode::protocols::universal_router::{
    uniswap_universal_router_deployments, EXECUTE_DEADLINE_SELECTOR, EXECUTE_SELECTOR,
    UNISWAP_UR_TABLE,
};
use crate::CallMatchKey;

use super::{SplitContext, SplitError, Splitter, SubCall};

// UR opcode constants we know how to fully decode at splitter time. Adding
// a new opcode here is the only place the splitter→mapper handshake needs
// to grow (the mapper itself lives in mappers/protocols/universal_router/).
const UR_OPCODE_V3_SWAP_EXACT_IN: u8 = 0x00;
const UR_OPCODE_V3_SWAP_EXACT_OUT: u8 = 0x01;
const UR_OPCODE_SWEEP: u8 = 0x04;
const UR_OPCODE_TRANSFER: u8 = 0x05;
const UR_OPCODE_V2_SWAP_EXACT_IN: u8 = 0x08;
const UR_OPCODE_V2_SWAP_EXACT_OUT: u8 = 0x09;
const UR_OPCODE_WRAP_ETH: u8 = 0x0b;
const UR_OPCODE_UNWRAP_WETH: u8 = 0x0c;
const UR_OPCODE_V4_SWAP: u8 = 0x10;

// Inline ABI decoders for the two UR `execute` overloads. The deadline
// overload is renamed because Rust can't host two `executeCall` types in
// one module; we feed the post-selector payload to `abi_decode_raw` to
// bypass the macro's strict selector check (the renamed call has a
// different synthetic selector than the on-chain 0x3593564c).
sol! {
    #[allow(clippy::too_many_arguments)]
    function execute(bytes commands, bytes[] inputs);
    #[allow(clippy::too_many_arguments)]
    function executeWithDeadline(bytes commands, bytes[] inputs, uint256 deadline);
}

/// One fork of a Universal-Router-style splitter. The dispatcher is shared
/// across forks; the fork-specific bits are the deployment allowlist and
/// the opcode table.
#[derive(Debug, Clone)]
pub struct UniversalRouterSplitter {
    id: &'static str,
    deployments: Vec<(u64, Address)>,
    opcode_table: &'static OpcodeTable,
}

impl UniversalRouterSplitter {
    /// Generic constructor. Prefer the `uniswap_ur()` / `pancake_ur()`
    /// factories below unless wiring a brand-new fork.
    #[must_use]
    pub fn new(
        id: &'static str,
        deployments: Vec<(u64, Address)>,
        opcode_table: &'static OpcodeTable,
    ) -> Self {
        Self {
            id,
            deployments,
            opcode_table,
        }
    }

    /// Pre-built splitter for Uniswap Universal Router.
    #[must_use]
    pub fn uniswap_ur() -> Self {
        let deployments = uniswap_universal_router_deployments()
            .map(|(chain_id, alloy_addr)| (chain_id, policy_address_from_alloy(&alloy_addr)))
            .collect();
        Self::new(
            "splitter/uniswap-universal-router",
            deployments,
            &UNISWAP_UR_TABLE,
        )
    }

    /// Pre-built splitter for PancakeSwap (Infinity) Universal Router.
    #[must_use]
    pub fn pancake_ur() -> Self {
        let deployments = pancake_universal_router_deployments()
            .map(|(chain_id, alloy_addr)| (chain_id, policy_address_from_alloy(&alloy_addr)))
            .collect();
        Self::new(
            "splitter/pancake-universal-router",
            deployments,
            &PANCAKE_UR_TABLE,
        )
    }

    pub fn id(&self) -> &'static str {
        self.id
    }

    /// Decode the outer `execute(...)` envelope and return `(commands, inputs)`.
    fn decode_outer_call(calldata: &[u8]) -> Result<(Vec<u8>, Vec<Vec<u8>>), SplitError> {
        if calldata.len() < 4 {
            return Err(SplitError::CalldataTooShort(calldata.len()));
        }
        let selector: [u8; 4] = calldata[..4]
            .try_into()
            .expect("checked length above ≥ 4 bytes");
        let payload = &calldata[4..];
        match selector {
            // `validate=false`: Uniswap's frontend appends attribution metadata
            // bytes after the standard ABI region. The EVM contract ignores them;
            // we do too. Strict mode would reject the calldata outright.
            EXECUTE_SELECTOR => {
                let call = executeCall::abi_decode_raw(payload, false)
                    .map_err(|e| SplitError::OuterDecode(format!("execute: {e}")))?;
                let inputs: Vec<Vec<u8>> = call.inputs.iter().map(|b| b.to_vec()).collect();
                Ok((call.commands.to_vec(), inputs))
            }
            EXECUTE_DEADLINE_SELECTOR => {
                let call = executeWithDeadlineCall::abi_decode_raw(payload, false)
                    .map_err(|e| SplitError::OuterDecode(format!("executeWithDeadline: {e}")))?;
                let inputs: Vec<Vec<u8>> = call.inputs.iter().map(|b| b.to_vec()).collect();
                Ok((call.commands.to_vec(), inputs))
            }
            _ => Err(SplitError::OuterDecode(format!(
                "unrecognised UR selector 0x{}",
                hex::encode(selector)
            ))),
        }
    }

    /// Turn one decoded opcode step into a SubCall. See the module doc for
    /// the calldata format we emit.
    ///
    /// When the opcode is one of the "fully understood" set (currently
    /// WRAP_ETH / UNWRAP_WETH; expanding in subsequent phases), the SubCall
    /// also carries a pre-decoded [`DecodedCall`] so downstream skips
    /// resolver+bridge and dispatches directly to the matching UR mapper.
    fn step_to_subcall(&self, ctx: &SplitContext<'_>, step: &DecodedStep) -> SubCall {
        let mut calldata = Vec::with_capacity(1 + step.raw_input.len());
        calldata.push(step.opcode);
        calldata.extend_from_slice(&step.raw_input);

        let decoded = pre_decode_for_opcode(step);

        SubCall {
            to: ctx.to.clone(),
            value_wei: ctx.value_wei.clone(),
            calldata,
            decoded,
        }
    }
}

/// Build a [`DecodedCall`] for an opcode the splitter fully understands.
/// Returns `None` for opcodes we haven't migrated yet — the SubCall still
/// carries the raw `[opcode | input]` calldata so a fallback path could
/// pick it up later.
fn pre_decode_for_opcode(step: &DecodedStep) -> Option<DecodedCall> {
    let decoder_id = match step.opcode {
        UR_OPCODE_V3_SWAP_EXACT_IN => UR_V3_SWAP_EXACT_IN_DECODER_ID,
        UR_OPCODE_V3_SWAP_EXACT_OUT => UR_V3_SWAP_EXACT_OUT_DECODER_ID,
        UR_OPCODE_SWEEP => UR_SWEEP_DECODER_ID,
        UR_OPCODE_TRANSFER => UR_TRANSFER_DECODER_ID,
        UR_OPCODE_V2_SWAP_EXACT_IN => UR_V2_SWAP_EXACT_IN_DECODER_ID,
        UR_OPCODE_V2_SWAP_EXACT_OUT => UR_V2_SWAP_EXACT_OUT_DECODER_ID,
        UR_OPCODE_WRAP_ETH => UR_WRAP_ETH_DECODER_ID,
        UR_OPCODE_UNWRAP_WETH => UR_UNWRAP_WETH_DECODER_ID,
        // V4_SWAP carries (bytes actions, bytes[] params); the splitter only
        // pre-decodes the outer wrapper and lets the mapper re-dispatch the
        // inner V4 action stream itself.
        UR_OPCODE_V4_SWAP => UR_V4_SWAP_DECODER_ID,
        _ => return None,
    };
    let args = step
        .args
        .as_ref()?
        .iter()
        .map(|legacy| DecodedArg {
            name: legacy.name.clone(),
            abi_type: legacy.sol_type.clone(),
            value: dyn_to_decoded(&legacy.value),
        })
        .collect();
    Some(DecodedCall {
        decoder_id: DecoderId::new(decoder_id),
        function_signature: format!("{}({})", step.name, "..."),
        args,
        nested: Vec::new(),
    })
}

/// Translate `DynSolValue` → `DecodedValue`. Mirrors `crate::bridge::convert_value`
/// in shape; we duplicate the dispatch table here so the splitter doesn't
/// depend on `bridge` (which is itself a thin compatibility shim).
fn dyn_to_decoded(value: &DynSolValue) -> DecodedValue {
    use std::str::FromStr as _;
    match value {
        DynSolValue::Address(addr) => {
            let hex = format!("0x{}", hex::encode(addr.0));
            DecodedValue::Address(Address::from_str(&hex).expect("alloy address renders as policy"))
        }
        DynSolValue::Uint(v, _) => DecodedValue::Uint(*v),
        DynSolValue::Int(v, _) => DecodedValue::Int(*v),
        DynSolValue::Bool(b) => DecodedValue::Bool(*b),
        DynSolValue::Bytes(b) => DecodedValue::Bytes(b.clone()),
        DynSolValue::FixedBytes(word, len) => DecodedValue::Bytes(word.as_slice()[..*len].to_vec()),
        DynSolValue::String(s) => DecodedValue::String(s.clone()),
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => {
            DecodedValue::Array(items.iter().map(dyn_to_decoded).collect())
        }
        DynSolValue::Tuple(items) => {
            DecodedValue::Tuple(items.iter().map(dyn_to_decoded).collect())
        }
        // Function values don't appear in UR opcode ABIs; fall back to empty
        // bytes if one slips through rather than panicking.
        DynSolValue::Function(_) => DecodedValue::Bytes(Vec::new()),
    }
}

impl Splitter for UniversalRouterSplitter {
    fn match_keys(&self) -> Vec<CallMatchKey> {
        let mut out = Vec::with_capacity(self.deployments.len() * 2);
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

    fn split(&self, ctx: &SplitContext<'_>, calldata: &[u8]) -> Result<Vec<SubCall>, SplitError> {
        let (commands, inputs) = Self::decode_outer_call(calldata)?;
        let steps = dispatch_opcodes(&commands, &inputs, self.opcode_table);
        let sub_calls = steps
            .iter()
            .map(|step| self.step_to_subcall(ctx, step))
            .collect();
        Ok(sub_calls)
    }
}

/// `Arc<dyn Splitter>` convenience for registry registration.
#[must_use]
pub fn uniswap_ur_arc() -> Arc<dyn Splitter> {
    Arc::new(UniversalRouterSplitter::uniswap_ur())
}

#[must_use]
pub fn pancake_ur_arc() -> Arc<dyn Splitter> {
    Arc::new(UniversalRouterSplitter::pancake_ur())
}

/// Mirror of `resolver::policy_address_from_alloy`, duplicated here so the
/// splitter doesn't depend on `crate::resolver` (private internal helper).
fn policy_address_from_alloy(addr: &alloy_primitives::Address) -> Address {
    format!("0x{}", hex::encode(addr.as_slice()))
        .parse()
        .expect("alloy address renders as a valid policy address")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function as AbiFunction;
    use alloy_primitives::{Address as AlloyAddress, U256};
    use policy_engine::action::DecimalString;

    fn addr(s: &str) -> Address {
        s.parse().unwrap()
    }
    fn dec(s: &str) -> DecimalString {
        s.parse().unwrap()
    }

    /// Encode a synthetic `execute(bytes,bytes[])` calldata blob. The
    /// `commands` argument is a packed opcode byte stream; each entry in
    /// `inputs` is the per-opcode raw bytes the dispatcher would feed to the
    /// per-opcode decoder.
    fn encode_execute(commands: &[u8], inputs: &[Vec<u8>]) -> Vec<u8> {
        let func = AbiFunction::parse("execute(bytes,bytes[])").unwrap();
        let values = vec![
            DynSolValue::Bytes(commands.to_vec()),
            DynSolValue::Array(
                inputs
                    .iter()
                    .map(|b| DynSolValue::Bytes(b.clone()))
                    .collect(),
            ),
        ];
        func.abi_encode_input(&values).unwrap()
    }

    #[test]
    fn uniswap_ur_factory_match_keys_non_empty() {
        let splitter = UniversalRouterSplitter::uniswap_ur();
        assert!(!splitter.match_keys().is_empty());
    }

    #[test]
    fn pancake_ur_factory_match_keys_non_empty() {
        let splitter = UniversalRouterSplitter::pancake_ur();
        assert!(!splitter.match_keys().is_empty());
    }

    #[test]
    fn uniswap_and_pancake_have_distinct_ids() {
        let u = UniversalRouterSplitter::uniswap_ur();
        let p = UniversalRouterSplitter::pancake_ur();
        assert_ne!(u.id(), p.id());
    }

    #[test]
    fn split_emits_one_subcall_per_opcode_step() {
        // Synthetic UR calldata: three commands, each with a trivial input.
        // The actual opcode semantics don't matter for this test — we just
        // need three distinct opcode bytes that exist in the Uniswap UR
        // table so dispatch produces three steps (UNKNOWN opcodes still
        // produce a step, but we'll use known opcodes for clarity).
        //
        // Pick: 0x0b WRAP_ETH, 0x0c UNWRAP_WETH, 0x04 SWEEP — all simple
        // (address, uint256) or (address, address, uint256) shapes.
        let wrap_input = encode_address_uint256([0x11; 20], 1_000);
        let unwrap_input = encode_address_uint256([0x22; 20], 2_000);
        let sweep_input = encode_address_address_uint256([0x33; 20], [0x44; 20], 3_000);

        let calldata = encode_execute(
            &[0x0b, 0x0c, 0x04],
            &[wrap_input, unwrap_input, sweep_input],
        );

        let splitter = UniversalRouterSplitter::uniswap_ur();
        let from = addr("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let to = addr("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let value = dec("0");
        let ctx = SplitContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
        };
        let sub_calls = splitter.split(&ctx, &calldata).unwrap();
        assert_eq!(sub_calls.len(), 3);

        // Each SubCall.to mirrors ctx.to (UR router itself).
        for s in &sub_calls {
            assert_eq!(s.to, to);
        }
        // Each SubCall.calldata starts with the opcode byte (our wire format).
        assert_eq!(sub_calls[0].calldata[0], 0x0b);
        assert_eq!(sub_calls[1].calldata[0], 0x0c);
        assert_eq!(sub_calls[2].calldata[0], 0x04);
    }

    #[test]
    fn split_rejects_calldata_shorter_than_selector() {
        let splitter = UniversalRouterSplitter::uniswap_ur();
        let from = addr("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let to = addr("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let value = dec("0");
        let ctx = SplitContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
        };
        let err = splitter.split(&ctx, &[0x35, 0x93]).unwrap_err();
        assert!(matches!(err, SplitError::CalldataTooShort(2)));
    }

    #[test]
    fn split_rejects_unknown_outer_selector() {
        let splitter = UniversalRouterSplitter::uniswap_ur();
        let from = addr("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let to = addr("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let value = dec("0");
        let ctx = SplitContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
        };
        // Selector 0xdeadbeef is neither execute() nor executeWithDeadline().
        let calldata = vec![0xde, 0xad, 0xbe, 0xef, 0x00];
        let err = splitter.split(&ctx, &calldata).unwrap_err();
        assert!(matches!(err, SplitError::OuterDecode(_)));
    }

    // Helpers (kept private to this test module) ----------------------------

    fn encode_address_uint256(a: [u8; 20], v: u128) -> Vec<u8> {
        let func = AbiFunction::parse("step(address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(AlloyAddress::from(a)),
            DynSolValue::Uint(U256::from(v), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        // strip the synthetic 4-byte selector to match what UR's inputs[i] holds.
        raw[4..].to_vec()
    }

    fn encode_address_address_uint256(a: [u8; 20], b: [u8; 20], v: u128) -> Vec<u8> {
        let func = AbiFunction::parse("step(address,address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(AlloyAddress::from(a)),
            DynSolValue::Address(AlloyAddress::from(b)),
            DynSolValue::Uint(U256::from(v), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }
}
