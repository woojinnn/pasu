//! End-to-end resolver tests using real-shaped calldata.
//!
//! These cover the public surface from the outside: build a `Resolver`,
//! hand it `(chain, address, calldata)`, expect a `Resolved` outcome with the
//! right function name and decoded argument values.

use abi_resolver::{
    decode::format_value,
    openchain::{OpenchainIndex, SignatureCandidate},
    resolver::{ResolveOutcome, Resolver, Source},
    sourcify::SourcifyIndex,
};
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

fn transfer_calldata(to: [u8; 20], amount: u128) -> Vec<u8> {
    let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
    let mut to_word = [0u8; 32];
    to_word[12..].copy_from_slice(&to);
    data.extend_from_slice(&to_word);
    data.extend_from_slice(&U256::from(amount).to_be_bytes::<32>());
    data
}

fn seeded_resolver() -> Resolver {
    let mut openchain = OpenchainIndex::empty();
    for (selector, signature) in [
        ([0x09u8, 0x5e, 0xa7, 0xb3], "approve(address,uint256)"),
        ([0xa9, 0x05, 0x9c, 0xbb], "transfer(address,uint256)"),
    ] {
        openchain.insert(
            selector,
            SignatureCandidate {
                signature: signature.into(),
                verified: true,
            },
        );
    }
    Resolver::new(SourcifyIndex::empty(), openchain)
}

#[test]
fn end_to_end_approve_via_openchain() {
    let resolver = seeded_resolver();
    let calldata = approve_calldata([0x11; 20], 100);

    match resolver.resolve(1, &Address::from([0x42; 20]), &calldata) {
        ResolveOutcome::Resolved(r) => {
            assert_eq!(r.source, Source::Openchain);
            assert_eq!(r.decoded.function_name, "approve");
            assert_eq!(r.decoded.signature, "approve(address,uint256)");
            assert_eq!(r.decoded.args.len(), 2);
            assert_eq!(format_value(&r.decoded.args[1].value), "100");
        }
        other => panic!("expected Resolved, got {other:?}"),
    }
}

#[test]
fn end_to_end_transfer_via_openchain() {
    let resolver = seeded_resolver();
    let calldata = transfer_calldata([0x33; 20], 250);

    match resolver.resolve(1, &Address::from([0x42; 20]), &calldata) {
        ResolveOutcome::Resolved(r) => {
            assert_eq!(r.source, Source::Openchain);
            assert_eq!(r.decoded.function_name, "transfer");
            assert_eq!(format_value(&r.decoded.args[1].value), "250");
        }
        other => panic!("expected Resolved, got {other:?}"),
    }
}

#[test]
fn sourcify_takes_priority_over_openchain() {
    // Both indices know approve. Sourcify carries parameter names; we should
    // see "spender"/"amount" rather than "arg0"/"arg1".
    let mut sourcify = SourcifyIndex::empty();
    let address = Address::from([0x42; 20]);
    let abi_json = serde_json::json!({
        "name": "approve",
        "type": "function",
        "inputs": [
            { "name": "spender", "type": "address" },
            { "name": "amount",  "type": "uint256" }
        ],
        "outputs": [{ "name": "", "type": "bool" }],
        "stateMutability": "nonpayable"
    });
    let function: Function = serde_json::from_value(abi_json).unwrap();
    sourcify.insert_contract(1, address, &[function]);

    let mut openchain = OpenchainIndex::empty();
    openchain.insert(
        [0x09, 0x5e, 0xa7, 0xb3],
        SignatureCandidate {
            signature: "approve(address,uint256)".into(),
            verified: true,
        },
    );

    let resolver = Resolver::new(sourcify, openchain);
    let calldata = approve_calldata([0x11; 20], 100);

    match resolver.resolve(1, &address, &calldata) {
        ResolveOutcome::Resolved(r) => {
            assert_eq!(r.source, Source::Sourcify, "should prefer Sourcify");
            assert_eq!(r.decoded.args[0].name, "spender");
            assert_eq!(r.decoded.args[1].name, "amount");
        }
        other => panic!("expected Resolved via Sourcify, got {other:?}"),
    }
}

#[test]
fn unknown_selector_returns_not_found() {
    let resolver = seeded_resolver();
    // Random selector not in the seed list.
    let mut calldata = vec![0xde, 0xad, 0xbe, 0xef];
    calldata.extend_from_slice(&[0u8; 64]);
    assert!(matches!(
        resolver.resolve(1, &Address::from([0x42; 20]), &calldata),
        ResolveOutcome::NotFound
    ));
}

#[test]
fn empty_resolver_returns_not_found_even_for_well_known_selectors() {
    let resolver = Resolver::empty();
    let calldata = approve_calldata([0x11; 20], 100);
    assert!(matches!(
        resolver.resolve(1, &Address::from([0x42; 20]), &calldata),
        ResolveOutcome::NotFound
    ));
}
