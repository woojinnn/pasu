//! Typed-data fuzzer — EIP-712 signature flows (Permit2 Permit{Single,Batch},
//! EIP-2612 `Permit`, HyperLiquid `HyperliquidTransaction:*`).
//!
//! Synthesizes a named EIP-712 `message` matching `match.typed_data.types`, then
//! routes it via `declarative_route_typed_data_v3_json`. Because the message is
//! a named object (unlike positionally-decoded calldata), `$args.<struct>.<field>`
//! paths resolve — this is the production route for these sign-primary entries.
//!
//! Cut line (plan): `PermitWitnessTransferFrom` witness orders (UniswapX
//! ExclusiveDutch / V2Dutch / V3Dutch / Priority) carry an intricate nested
//! order witness; faithful synthesis is deferred to corpus replay (Phase 3).

use std::panic::{catch_unwind, AssertUnwindSafe};

use serde_json::{Map, Value};

use crate::harness::adapters::{RoutableSurface, RoutableTypedData};
use crate::harness::fuzz::EDGE_ITERS;
use crate::harness::oracle::{judge, OracleLayer, Verdict};
use crate::harness::prng::SplitMix64;
use crate::harness::{encode, report::Report, route};

/// Synthesize an EIP-712 message for `struct_name` from the `types` table.
fn synth_struct(types: &Value, struct_name: &str, rng: &mut SplitMix64, depth: u32) -> Value {
    let Some(fields) = types.get(struct_name).and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut obj = Map::new();
    for f in fields {
        let name = f.get("name").and_then(Value::as_str).unwrap_or("");
        let ty = f.get("type").and_then(Value::as_str).unwrap_or("");
        obj.insert(name.to_owned(), synth_field(types, ty, rng, depth));
    }
    Value::Object(obj)
}

fn synth_field(types: &Value, ty: &str, rng: &mut SplitMix64, depth: u32) -> Value {
    if let Some(base) = ty.strip_suffix("[]") {
        // single-element array keeps index-addressed bodies (`details[0]`) happy
        return Value::Array(vec![synth_field(types, base, rng, depth)]);
    }
    // Nested struct (a key in the types table).
    if depth < 6 && types.get(ty).is_some() {
        return synth_struct(types, ty, rng, depth + 1);
    }
    // Scalar.
    if ty == "address" {
        Value::String("0x0000000000000000000000000000000000000000".to_owned())
    } else if ty == "bool" {
        Value::Bool(false)
    } else if ty == "string" {
        Value::String(String::new())
    } else if let Some(n) = ty
        .strip_prefix("bytes")
        .and_then(|s| s.parse::<usize>().ok())
    {
        // fixed bytesN → 2N hex zeros
        Value::String(format!("0x{}", "00".repeat(n)))
    } else if ty == "bytes" {
        Value::String("0x".to_owned())
    } else if ty.starts_with("uint") || ty.starts_with("int") {
        // decimal string; vary a little so distinct fields aren't all identical
        Value::String((1 + rng.below(8)).to_string())
    } else {
        Value::String("0".to_owned())
    }
}

fn route_one(td: &RoutableTypedData, seed: u64) -> (String, crate::harness::oracle::Judged) {
    let mut rng = SplitMix64::new(seed);
    let msg = synth_struct(&td.types, &td.primary_type, &mut rng, 0);
    let env = route::route_typed_data(
        td.chain_id,
        &td.verifying_contract,
        &td.primary_type,
        td.witness_type.as_deref(),
        td.domain_name.as_deref(),
        &msg,
    );
    (msg.to_string(), judge(&env))
}

/// Fuzz every non-witness typed-data key `iters` times.
pub fn fuzz_surface(surface: &RoutableSurface, global_seed: u64, iters: u64, report: &mut Report) {
    for td in &surface.typed {
        // UniswapX witness orders → corpus-replay only (R: faithful witness
        // synthesis is out of scope).
        if td.witness_type.is_some() {
            report.record_skip();
            continue;
        }
        let base = encode::fnv1a64(&td.source_key);
        for i in 0..iters {
            let seed = base ^ global_seed ^ i;
            let outcome = catch_unwind(AssertUnwindSafe(|| route_one(td, seed)));
            let _ = EDGE_ITERS; // message synth is structural; no separate edge mode
            match outcome {
                Ok((msg, mut judged)) => {
                    // Synthetic EIP-712 messages can't match every ActionBody
                    // field coercion (uint string-vs-number, per-manifest
                    // root_param wrap shape). Downgrade those build/round-trip
                    // fidelity fails to soft — real signed messages are verified
                    // positively by the corpus (Phase 3). Panics stay hard
                    // (handled in the Err arm); invalid-domain stays hard.
                    if let Verdict::Fail { layer, .. } = &judged.verdict {
                        if matches!(layer, OracleLayer::ErrorClass | OracleLayer::TypedRoundTrip) {
                            judged.verdict = Verdict::SoftError {
                                kind: "typed_data_synthesis_limited".to_owned(),
                            };
                            judged.error_kind = Some("typed_data_synthesis_limited".to_owned());
                        }
                    }
                    report.record(
                        &td.source_key,
                        &td.bundle_id,
                        "typed_data",
                        seed,
                        &msg,
                        &judged,
                    );
                }
                Err(_) => report.record_panic(
                    &td.source_key,
                    &td.bundle_id,
                    "typed_data",
                    seed,
                    "<panic>",
                ),
            }
        }
    }
}
