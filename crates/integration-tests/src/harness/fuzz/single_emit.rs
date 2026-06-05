//! `single_emit` fuzzer — flat ABI args → calldata → route → oracle.
//!
//! Covers the bulk of the surface (ERC-standard tokens, Aave Pool, single-fn
//! adapters). Any type-valid input MUST decode to a well-formed `ActionBody`,
//! so a hard engine error here is a finding.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::str::FromStr as _;

use alloy_dyn_abi::DynSolValue;
use alloy_primitives::{Address, U256};
use anyhow::Result;

use crate::harness::adapters::{RoutableCall, RoutableSurface, Strategy};
use crate::harness::fuzz::values::{abi_input_to_soltype, gen_value, Edge};
use crate::harness::fuzz::EDGE_ITERS;
use crate::harness::oracle::{judge, Judged};
use crate::harness::{encode, route};

/// Build args + calldata for one iteration, route it, and judge the envelope.
/// Returns `(calldata, judged)`, or `Err` if the harness could not build args
/// for this ABI (a harness skip, not a decode finding).
pub fn fuzz_one(call: &RoutableCall, seed: u64, edge: Edge) -> Result<(String, Judged)> {
    let calldata = build_calldata(call, seed, edge)?;
    let env = route::route_calldata(call.chain_id, &call.to, &call.selector, &calldata, "0");
    Ok((calldata, judge(&env)))
}

/// Build the `0x`-prefixed calldata for one fuzz iteration (no routing). Used by
/// [`fuzz_one`] and by the CLI `replay` command.
pub fn build_calldata(call: &RoutableCall, seed: u64, edge: Edge) -> Result<String> {
    let args = if let Some(args) = uniswapx_reactor_args(call)? {
        args
    } else {
        let mut rng = crate::harness::prng::SplitMix64::new(seed);
        call.abi_inputs
            .iter()
            .map(|i| Ok(gen_value(&mut rng, &abi_input_to_soltype(i)?, edge)))
            .collect::<Result<Vec<DynSolValue>>>()?
    };
    Ok(encode::encode_calldata(&call.selector, &args))
}

fn uniswapx_reactor_args(call: &RoutableCall) -> Result<Option<Vec<DynSolValue>>> {
    if !call.bundle_id.starts_with("uniswapx/reactor/") {
        return Ok(None);
    }
    let Some(order_bytes) = uniswapx_sample_order_bytes(&call.to) else {
        return Ok(None);
    };
    let signed_order = DynSolValue::Tuple(vec![
        DynSolValue::Bytes(order_bytes),
        DynSolValue::Bytes(vec![0xab; 65]),
    ]);
    let args = match call.selector.as_str() {
        "0x3f62192e" => vec![signed_order],
        "0x0d335884" => vec![signed_order, DynSolValue::Bytes(vec![0xcd])],
        _ => return Ok(None),
    };
    Ok(Some(args))
}

fn uniswapx_sample_order_bytes(reactor: &str) -> Option<Vec<u8>> {
    let lower = reactor.to_ascii_lowercase();
    let family = match lower.as_str() {
        "0x6000da47483062a0d734ba3dc7576ce6a0b645c4" => UniswapXFamily::ExclusiveDutch,
        "0x00000011f84b9aa48e5f8aa8b9897600006289be"
        | "0x1bd1aadc8a99fe9c48cffd9a5718b67f83cd4c08" => UniswapXFamily::V2Dutch,
        "0x0000000015757c461808ea25eb309638b62681cf"
        | "0x000000008a8330b5e401f8d6b6f4d82e9e6fef4a"
        | "0x000000000923439a00000cfd2e0c5e60fef971c4"
        | "0xb274d5f4b833b61b340b654d600a864fb604a87c" => UniswapXFamily::V3Dutch,
        "0x000000001ec5656dcdb24d90dfa42742738de729" => UniswapXFamily::Priority,
        _ => return None,
    };
    Some(encode_uniswapx_order(family, &lower))
}

#[derive(Clone, Copy)]
enum UniswapXFamily {
    ExclusiveDutch,
    V2Dutch,
    V3Dutch,
    Priority,
}

fn encode_uniswapx_order(family: UniswapXFamily, reactor: &str) -> Vec<u8> {
    let reactor = address(reactor);
    let zero = Address::ZERO;
    let info = DynSolValue::Tuple(vec![
        DynSolValue::Address(reactor),
        DynSolValue::Address(address("0x1111111111111111111111111111111111111111")),
        uint(42),
        uint(1_900_000_000),
        DynSolValue::Address(zero),
        DynSolValue::Bytes(Vec::new()),
    ]);
    let sell = address("0x2222222222222222222222222222222222222222");
    let buy = address("0x3333333333333333333333333333333333333333");
    let recipient = address("0x4444444444444444444444444444444444444444");

    let order = match family {
        UniswapXFamily::ExclusiveDutch => DynSolValue::Tuple(vec![
            info,
            uint(1_800_000_000),
            uint(1_900_000_000),
            DynSolValue::Address(zero),
            uint(0),
            DynSolValue::Tuple(vec![DynSolValue::Address(sell), uint(1_000), uint(1_100)]),
            DynSolValue::Array(vec![DynSolValue::Tuple(vec![
                DynSolValue::Address(buy),
                uint(2_000),
                uint(1_800),
                DynSolValue::Address(recipient),
            ])]),
        ]),
        UniswapXFamily::V2Dutch => DynSolValue::Tuple(vec![
            info,
            DynSolValue::Address(zero),
            DynSolValue::Tuple(vec![DynSolValue::Address(sell), uint(1_000), uint(1_100)]),
            DynSolValue::Array(vec![DynSolValue::Tuple(vec![
                DynSolValue::Address(buy),
                uint(2_000),
                uint(1_800),
                DynSolValue::Address(recipient),
            ])]),
            DynSolValue::Tuple(vec![
                uint(1_800_000_000),
                uint(1_900_000_000),
                DynSolValue::Address(zero),
                uint(0),
                uint(900),
                DynSolValue::Array(vec![uint(0)]),
            ]),
            DynSolValue::Bytes(vec![0xef; 65]),
        ]),
        UniswapXFamily::V3Dutch => {
            let curve = DynSolValue::Tuple(vec![uint(0), DynSolValue::Array(vec![int(0)])]);
            DynSolValue::Tuple(vec![
                info,
                DynSolValue::Address(zero),
                uint(1_000_000_000),
                DynSolValue::Tuple(vec![
                    DynSolValue::Address(sell),
                    uint(1_000),
                    curve.clone(),
                    uint(1_200),
                    uint(0),
                ]),
                DynSolValue::Array(vec![DynSolValue::Tuple(vec![
                    DynSolValue::Address(buy),
                    uint(2_000),
                    curve,
                    DynSolValue::Address(recipient),
                    uint(1_800),
                    uint(0),
                ])]),
                DynSolValue::Tuple(vec![
                    uint(1_800_000_000),
                    DynSolValue::Address(zero),
                    uint(0),
                    uint(900),
                    DynSolValue::Array(vec![uint(0)]),
                ]),
                DynSolValue::Bytes(vec![0xef; 65]),
            ])
        }
        UniswapXFamily::Priority => DynSolValue::Tuple(vec![
            info,
            DynSolValue::Address(zero),
            uint(12_345_678),
            uint(1_000_000_000),
            DynSolValue::Tuple(vec![DynSolValue::Address(sell), uint(1_000), uint(0)]),
            DynSolValue::Array(vec![DynSolValue::Tuple(vec![
                DynSolValue::Address(buy),
                uint(1_800),
                uint(0),
                DynSolValue::Address(recipient),
            ])]),
            DynSolValue::Tuple(vec![uint(12_345_690)]),
            DynSolValue::Bytes(vec![0xef; 65]),
        ]),
    };

    DynSolValue::Tuple(vec![order]).abi_encode_params()
}

#[allow(clippy::expect_used)] // fuzz fixture: parses a known-valid sample literal
fn address(value: &str) -> Address {
    Address::from_str(value).expect("sample address")
}

fn uint(value: u64) -> DynSolValue {
    DynSolValue::Uint(U256::from(value), 256)
}

#[allow(clippy::expect_used)] // fuzz fixture: i64 fits an i256 sample literal
fn int(value: i64) -> DynSolValue {
    DynSolValue::Int(value.try_into().expect("sample int"), 256)
}

/// Fuzz every `single_emit` callkey on the surface `iters` times each.
///
/// Seed per iteration = `fnv1a64(callkey) ^ global_seed ^ i` (position-stable,
/// replayable). The first [`EDGE_ITERS`] iterations use boundary values.
pub fn fuzz_surface(
    surface: &RoutableSurface,
    global_seed: u64,
    iters: u64,
    report: &mut crate::harness::report::Report,
) {
    // Calldata-routable single_emit only. Entries with `match.typed_data` are
    // sign-primary (sentinel callkey selectors + named EIP-712 message bodies)
    // and are exercised by the typed-data fuzzer (Phase 2) instead.
    for call in surface.calls.iter().filter(|c| {
        c.strategy == Strategy::SingleEmit
            && !c.has_typed_data
            // `0x00000000` is the selector-less / native-transfer (and synthetic)
            // sentinel — it needs empty calldata + value, not generic ABI-encoded
            // calldata. Covered by a dedicated native-transfer path, not here.
            && c.selector != "0x00000000"
    }) {
        let base = encode::fnv1a64(&call.source_callkey);
        for i in 0..iters {
            let seed = base ^ global_seed ^ i;
            let edge = if i < EDGE_ITERS {
                Edge::Edge
            } else {
                Edge::Random
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| fuzz_one(call, seed, edge)));
            match outcome {
                Ok(Ok((calldata, judged))) => report.record(
                    &call.source_callkey,
                    &call.bundle_id,
                    "single_emit",
                    seed,
                    &calldata,
                    &judged,
                ),
                Ok(Err(_)) => report.record_skip(),
                Err(_) => report.record_panic(
                    &call.source_callkey,
                    &call.bundle_id,
                    "single_emit",
                    seed,
                    "<panic before calldata>",
                ),
            }
        }
    }
}
