//! Walk `swap_samples/uniswap/**/samples.jsonl`, dispatch each line through
//! `mappers::registry::dispatch`, assemble into a `RootRequest`, and emit JSON.
//!
//! Per-folder success/fail counts are printed at the end. One sample JSON
//! per folder is printed to stdout for spot-check.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use abi_resolver::decode::DecodedCall;
use abi_resolver::openchain::OpenchainIndex;
use abi_resolver::resolver::{ResolveOutcome, Resolver};
use abi_resolver::sourcify::SourcifyIndex;
use alloy_primitives::Address;
use mappers::assembler::assemble;
use mappers::context::{BuildContext, RawTx};
use mappers::registry::{dispatch, protocol_for};
use serde_json::Value;

const BASE: &str = "swap_samples/uniswap";
const SOURCIFY_BUNDLE: &[u8] = include_bytes!("../../abi-resolver/data/sourcify.json");

fn main() -> std::io::Result<()> {
    let project_root = std::env::current_dir()?;
    // Walk up to find the project root (containing swap_samples/)
    let mut root = project_root.clone();
    while !root.join(BASE).exists() {
        if !root.pop() {
            eprintln!(
                "could not find {BASE} starting from {}",
                project_root.display()
            );
            std::process::exit(1);
        }
    }
    let base = root.join(BASE);
    eprintln!("Walking: {}", base.display());

    let sourcify =
        SourcifyIndex::load_bundle(SOURCIFY_BUNDLE).expect("curated sourcify bundle must parse");
    let resolver = Resolver::new(sourcify, OpenchainIndex::empty());

    let mut totals: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut printed_examples: BTreeMap<String, bool> = BTreeMap::new();

    walk_jsonl(&base, &mut |jsonl_path| {
        let rel = jsonl_path
            .strip_prefix(&base)
            .unwrap()
            .parent()
            .unwrap()
            .display()
            .to_string();
        let body = fs::read_to_string(jsonl_path).unwrap_or_default();
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(rec) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(tx) = build_raw_tx(&rec) else {
                continue;
            };
            let ctx = BuildContext {
                chain_id: 1,
                block_timestamp: 0,
                ..Default::default()
            };
            let entry = totals.entry(rel.clone()).or_insert((0, 0, 0));
            // Resolve via abi-resolver so migrated mappers can read decoded
            // args by name. Legacy mappers ignore `call` and still sol!-decode
            // `tx.input` themselves, so a miss here only hurts migrated
            // selectors.
            let call = resolve_call(&resolver, &tx).unwrap_or_else(empty_call);
            match dispatch(&ctx, &tx, &call) {
                Ok(envelopes) if !envelopes.is_empty() => {
                    entry.0 += 1;
                    if !printed_examples.contains_key(&rel) {
                        let root =
                            assemble(&tx, &ctx, protocol_for(&tx.to.to_lowercase()), envelopes);
                        let json = serde_json::to_string_pretty(&root).unwrap();
                        println!("─── {} ───", rel);
                        println!("{}", json);
                        println!();
                        printed_examples.insert(rel.clone(), true);
                    }
                }
                Ok(_) => entry.1 += 1, // decoded but no policy-relevant envelope
                Err(_) => entry.2 += 1, // dispatch error (unsupported, decode failure)
            }
        }
    });

    eprintln!(
        "\n=== Summary (ok=produced envelopes, empty=no swap/wrap action, err=decode failure) ==="
    );
    let mut total_ok = 0usize;
    let mut total_empty = 0usize;
    let mut total_err = 0usize;
    for (folder, (ok, empty, err)) in &totals {
        eprintln!(
            "  {:60}  ok={:>3}  empty={:>3}  err={:>3}",
            folder, ok, empty, err
        );
        total_ok += ok;
        total_empty += empty;
        total_err += err;
    }
    eprintln!(
        "\nTotal: ok={} empty={} err={}",
        total_ok, total_empty, total_err
    );
    Ok(())
}

fn walk_jsonl(dir: &Path, on_file: &mut dyn FnMut(&Path)) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_jsonl(&p, on_file);
        } else if p.file_name().and_then(|s| s.to_str()) == Some("samples.jsonl") {
            on_file(&p);
        }
    }
}

fn build_raw_tx(rec: &Value) -> Option<RawTx> {
    let to = rec.get("to")?.as_str()?.to_string();
    let from = rec.get("from")?.as_str()?.to_string();
    let input_hex = rec.get("input")?.as_str()?;
    let value = rec
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("0")
        .to_string();
    let stripped = input_hex.strip_prefix("0x").unwrap_or(input_hex);
    let input = hex::decode(stripped).ok()?;
    Some(RawTx {
        chain_id: 1,
        from,
        to,
        value,
        input,
    })
}

fn resolve_call(resolver: &Resolver, tx: &RawTx) -> Option<DecodedCall> {
    let address = Address::from_str(&tx.to).ok()?;
    match resolver.resolve(tx.chain_id, &address, &tx.input) {
        ResolveOutcome::Resolved(r) => Some(r.decoded),
        ResolveOutcome::NotFound => None,
    }
}

/// Placeholder call for samples whose selector isn't in Sourcify. Legacy
/// mappers ignore the `call` argument anyway; migrated mappers should see a
/// real Resolver hit for any selector they handle.
fn empty_call() -> DecodedCall {
    DecodedCall {
        function_name: String::new(),
        signature: String::new(),
        args: Vec::new(),
    }
}
