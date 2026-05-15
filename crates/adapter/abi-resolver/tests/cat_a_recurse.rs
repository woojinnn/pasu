//! Recursive sub-call extraction integration tests.
//!
//! Builds a real `multicall(bytes[])` calldata wrapping two `approve(...)`
//! sub-calls, then verifies the resolver + sub-decode helpers produce the
//! tree the orchestrator expects.

use abi_resolver::{
    openchain::{OpenchainIndex, SignatureCandidate},
    resolver::{ResolveOutcome, Resolver},
    sourcify::SourcifyIndex,
    subdecode::recurse::{extract_subcalls, is_self_multicall, MULTICALL_BYTES_ARRAY},
};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::{Address, U256};

fn approve_calldata(spender: [u8; 20], amount: u128) -> Vec<u8> {
    let mut data = vec![0x09, 0x5e, 0xa7, 0xb3];
    let mut spender_word = [0u8; 32];
    spender_word[12..].copy_from_slice(&spender);
    data.extend_from_slice(&spender_word);
    data.extend_from_slice(&U256::from(amount).to_be_bytes::<32>());
    data
}

fn multicall_calldata(subcalls: Vec<Vec<u8>>) -> Vec<u8> {
    let func = Function::parse("multicall(bytes[])").unwrap();
    let value = DynSolValue::Array(
        subcalls
            .into_iter()
            .map(DynSolValue::Bytes)
            .collect::<Vec<_>>(),
    );
    // `abi_encode_input` already prefixes the 4-byte selector.
    func.abi_encode_input(&[value]).unwrap()
}

fn seeded_resolver() -> Resolver {
    let mut openchain = OpenchainIndex::empty();
    openchain.insert(
        MULTICALL_BYTES_ARRAY,
        SignatureCandidate {
            signature: "multicall(bytes[])".into(),
            verified: true,
        },
    );
    openchain.insert(
        [0x09, 0x5e, 0xa7, 0xb3],
        SignatureCandidate {
            signature: "approve(address,uint256)".into(),
            verified: true,
        },
    );
    Resolver::new(SourcifyIndex::empty(), openchain)
}

#[test]
fn extract_then_recurse_yields_two_approves() {
    let inner1 = approve_calldata([0x11; 20], 100);
    let inner2 = approve_calldata([0x22; 20], 200);
    let outer = multicall_calldata(vec![inner1.clone(), inner2.clone()]);

    let resolver = seeded_resolver();
    let target = Address::from([0xaa; 20]);

    // Outer decode.
    let outer_decoded = match resolver.resolve(1, &target, &outer) {
        ResolveOutcome::Resolved(r) => r.decoded,
        other => panic!("expected outer multicall to resolve, got {other:?}"),
    };
    assert_eq!(outer_decoded.function_name, "multicall");

    // Selector identification.
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&outer[..4]);
    assert!(is_self_multicall(&selector));

    // Subcall extraction.
    let subcalls = extract_subcalls(&outer_decoded).expect("multicall should expose bytes[]");
    assert_eq!(subcalls.len(), 2);
    assert_eq!(subcalls[0], inner1);
    assert_eq!(subcalls[1], inner2);

    // Recursion: each subcall resolves to approve.
    for sub in &subcalls {
        match resolver.resolve(1, &target, sub) {
            ResolveOutcome::Resolved(r) => {
                assert_eq!(r.decoded.function_name, "approve");
            }
            other => panic!("expected approve sub-resolve, got {other:?}"),
        }
    }
}

#[test]
fn empty_multicall_extracts_to_empty_vec() {
    let outer = multicall_calldata(vec![]);
    let resolver = seeded_resolver();
    let target = Address::from([0xaa; 20]);

    let decoded = match resolver.resolve(1, &target, &outer) {
        ResolveOutcome::Resolved(r) => r.decoded,
        other => panic!("expected outer multicall to resolve, got {other:?}"),
    };
    let subcalls = extract_subcalls(&decoded).unwrap();
    assert!(subcalls.is_empty());
}

#[test]
fn non_multicall_selector_is_not_self_multicall() {
    let approve = approve_calldata([0x11; 20], 1);
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&approve[..4]);
    assert!(!is_self_multicall(&selector));
}

#[test]
#[ignore = "manual hex dump for QA — run with --ignored --nocapture"]
fn print_sample_multicall_hex_for_qa() {
    let inner1 = approve_calldata([0x11; 20], 100);
    let inner2 = approve_calldata([0x22; 20], 200);
    let outer = multicall_calldata(vec![inner1, inner2]);
    println!("\n== sample V3 SwapRouter multicall(bytes[]) ==");
    println!("chain_id : 1");
    println!("to       : 0xE592427A0AEce92De3Edee1F18E0157C05861564");
    println!("calldata : 0x{}", hex::encode(&outer));
}
