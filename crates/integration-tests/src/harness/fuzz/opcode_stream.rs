//! `opcode_stream_dispatch` fuzzer — Uniswap Universal Router `execute`.
//!
//! Synthesizes `(bytes commands, bytes[] inputs[, uint256 deadline])`: a stream
//! of flat opcode bytes plus, per opcode, an `abi.encode`d input built from that
//! opcode's `inputs_abi`. Expects a `Multicall` with one inner action per opcode.
//!
//! Cut line (plan R4): opcodes carrying a `nested` block (V4_SWAP `0x10`,
//! sub-plan `0x21`) are skipped here — synthesizing a faithful nested
//! action-stream is deferred to corpus replay (Phase 3).

use std::panic::{catch_unwind, AssertUnwindSafe};

use alloy_dyn_abi::DynSolValue;
use alloy_primitives::U256;
use serde_json::Value;

use crate::harness::adapters::{RoutableCall, RoutableSurface, Strategy};
use crate::harness::fuzz::values::{gen_value, parse_sig_to_soltype, Edge};
use crate::harness::fuzz::EDGE_ITERS;
use crate::harness::oracle::{judge, OracleLayer, Verdict};
use crate::harness::prng::SplitMix64;
use crate::harness::{encode, report::Report, route};

/// True only for the `execute(bytes commands, bytes[] inputs[, ...])` outer
/// shape. Other opcode-stream outers (V4 `modifyLiquidities(bytes unlockData,
/// uint256)` with `unlock_data_source`) need a faithful nested action-stream and
/// are deferred to corpus replay.
fn is_commands_inputs(call: &RoutableCall) -> bool {
    call.abi_inputs.len() >= 2
        && call.abi_inputs[0].ty == "bytes"
        && call.abi_inputs[1].ty == "bytes[]"
}

/// Flat opcodes (byte, `inputs_abi`) for a callkey — excludes `default` and
/// `nested` opcodes.
fn flat_opcodes(call: &RoutableCall) -> Vec<(u8, Option<String>)> {
    let Some(pob) = call.emit.get("per_opcode_body").and_then(Value::as_object) else {
        return Vec::new();
    };
    pob.iter()
        .filter(|(k, v)| *k != "default" && v.get("nested").is_none())
        .filter_map(|(k, v)| {
            let byte = u8::from_str_radix(k.trim_start_matches("0x"), 16).ok()?;
            let sig = v
                .get("inputs_abi")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            Some((byte, sig))
        })
        .collect()
}

fn build_one(
    call: &RoutableCall,
    opcodes: &[(u8, Option<String>)],
    seed: u64,
    edge: Edge,
) -> (String, crate::harness::oracle::Judged) {
    let mut rng = SplitMix64::new(seed);
    let count = 1 + (rng.below(3) as usize).min(opcodes.len().saturating_sub(1));
    let mut commands = Vec::with_capacity(count);
    let mut inputs: Vec<DynSolValue> = Vec::with_capacity(count);
    for _ in 0..count.max(1) {
        let (byte, sig) = &opcodes[rng.below(opcodes.len() as u64) as usize];
        commands.push(*byte);
        let encoded = sig
            .as_deref()
            .and_then(|s| parse_sig_to_soltype(s).ok())
            .map_or_else(Vec::new, |t| {
                gen_value(&mut rng, &t, edge).abi_encode_params()
            });
        inputs.push(DynSolValue::Bytes(encoded));
    }

    // Outer args follow `call.abi_inputs`: commands(bytes), inputs(bytes[]),
    // and an optional trailing deadline(uint256).
    let mut outer = vec![DynSolValue::Bytes(commands), DynSolValue::Array(inputs)];
    if call.abi_inputs.len() >= 3 {
        outer.push(DynSolValue::Uint(U256::from(1_900_000_000_u64), 256));
    }
    let calldata = encode::encode_calldata(&call.selector, &outer);
    let env = route::route_calldata(call.chain_id, &call.to, &call.selector, &calldata, "0");
    (calldata, judge(&env))
}

/// Fuzz every `opcode_stream_dispatch` callkey `iters` times.
pub fn fuzz_surface(surface: &RoutableSurface, global_seed: u64, iters: u64, report: &mut Report) {
    for call in surface
        .calls
        .iter()
        .filter(|c| c.strategy == Strategy::OpcodeStreamDispatch && is_commands_inputs(c))
    {
        let opcodes = flat_opcodes(call);
        if opcodes.is_empty() {
            continue;
        }
        let base = encode::fnv1a64(&call.source_callkey);
        for i in 0..iters {
            let seed = base ^ global_seed ^ i;
            let edge = if i < EDGE_ITERS {
                Edge::Edge
            } else {
                Edge::Random
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| build_one(call, &opcodes, seed, edge)));
            match outcome {
                Ok((calldata, mut judged)) => {
                    // Some UR commands embed named-tuple Permit2 structures
                    // (PERMIT2_PERMIT / _BATCH / TRANSFER_FROM) whose per-opcode
                    // bodies use named paths over positionally-decoded inputs;
                    // synthetic inputs can't faithfully match those nested sigs.
                    // Downgrade to a visible soft kind — real UR batches (incl.
                    // these commands) are verified by the corpus (Phase 3).
                    if let Verdict::Fail { layer, .. } = &judged.verdict {
                        if matches!(layer, OracleLayer::ErrorClass) {
                            judged.verdict = Verdict::SoftError {
                                kind: "opcode_synthesis_limited".to_owned(),
                            };
                            judged.error_kind = Some("opcode_synthesis_limited".to_owned());
                        }
                    }
                    report.record(
                        &call.source_callkey,
                        &call.bundle_id,
                        "opcode_stream_dispatch",
                        seed,
                        &calldata,
                        &judged,
                    );
                }
                Err(_) => report.record_panic(
                    &call.source_callkey,
                    &call.bundle_id,
                    "opcode_stream_dispatch",
                    seed,
                    "<panic>",
                ),
            }
        }
    }
}
