//! `tagged_dispatch` fuzzer — HyperLiquid CoreWriter `sendRawAction(bytes)`.
//!
//! The inner `data` = `version_byte ‖ action_id(uint24 BE) ‖ abi.encode(args)`.
//! For each `per_action_body` action id, synthesize the inner byte string and
//! route it as `sendRawAction(data)` calldata. Most actions map to `unknown`
//! bodies (off-chain L1 ops); id 11 (cancel-by-cloid) maps to perp.

use std::panic::{catch_unwind, AssertUnwindSafe};

use alloy_dyn_abi::DynSolValue;
use serde_json::Value;

use crate::harness::adapters::{RoutableCall, RoutableSurface, Strategy};
use crate::harness::fuzz::values::{gen_value, parse_sig_to_soltype, Edge};
use crate::harness::fuzz::EDGE_ITERS;
use crate::harness::oracle::judge;
use crate::harness::prng::SplitMix64;
use crate::harness::{encode, report::Report, route};

fn parse_u8_hex(v: Option<&Value>, default: u8) -> u8 {
    v.and_then(Value::as_str)
        .and_then(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(default)
}

/// Build the inner CoreWriter `data` for one action id, then route it.
fn route_action(
    call: &RoutableCall,
    version: u8,
    action_id: u32,
    sig: Option<&str>,
    seed: u64,
    edge: Edge,
) -> (String, crate::harness::oracle::Judged) {
    let mut rng = SplitMix64::new(seed);
    let mut data = vec![version];
    // uint24 big-endian action id (3 bytes).
    data.extend_from_slice(&action_id.to_be_bytes()[1..]);
    if let Some(sig) = sig {
        if let Ok(tuple) = parse_sig_to_soltype(sig) {
            data.extend_from_slice(&gen_value(&mut rng, &tuple, edge).abi_encode_params());
        }
    }
    let calldata = encode::encode_calldata(&call.selector, &[DynSolValue::Bytes(data)]);
    let env = route::route_calldata(call.chain_id, &call.to, &call.selector, &calldata, "0");
    (calldata, judge(&env))
}

/// Fuzz every `tagged_dispatch` callkey: every action id × `iters`.
pub fn fuzz_surface(surface: &RoutableSurface, global_seed: u64, iters: u64, report: &mut Report) {
    for call in surface
        .calls
        .iter()
        .filter(|c| c.strategy == Strategy::TaggedDispatch)
    {
        let version = parse_u8_hex(call.emit.get("version_byte"), 0x01);
        let Some(pab) = call.emit.get("per_action_body").and_then(Value::as_object) else {
            continue;
        };
        for (aid, entry) in pab {
            if aid == "default" {
                continue;
            }
            let Ok(action_id) = aid.parse::<u32>() else {
                continue;
            };
            let sig = entry.get("inputs_abi").and_then(Value::as_str);
            let base = encode::fnv1a64(&format!("{}#{aid}", call.source_callkey));
            for i in 0..iters {
                let seed = base ^ global_seed ^ i;
                let edge = if i < EDGE_ITERS {
                    Edge::Edge
                } else {
                    Edge::Random
                };
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    route_action(call, version, action_id, sig, seed, edge)
                }));
                let label = format!("{}#{aid}", call.source_callkey);
                match outcome {
                    Ok((calldata, judged)) => {
                        report.record(
                            &label,
                            &call.bundle_id,
                            "tagged_dispatch",
                            seed,
                            &calldata,
                            &judged,
                        );
                    }
                    Err(_) => report.record_panic(
                        &label,
                        &call.bundle_id,
                        "tagged_dispatch",
                        seed,
                        "<panic>",
                    ),
                }
            }
        }
    }
}
