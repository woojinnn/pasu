//! `v3-harness` — CLI for the v3 `ActionBody[]` decode harness.
//!
//! Drives the production decode entrypoints over the local `registryV2/index`
//! surface — no browser, no WASM runtime, no RPC. Subcommands:
//!
//! ```text
//! v3-harness fuzz     [--iterations N] [--seed S] [--json PATH]
//! v3-harness coverage
//! v3-harness replay   --callkey <chain>__<addr>__<selector> [--seed S]
//! ```
//!
//! `fuzz` runs the all-strategy synthetic sweep and prints the report (optionally
//! dumping the full JSON for CI). `coverage` prints the routable surface broken
//! down by strategy + the categories deferred to corpus replay. `replay`
//! reproduces a single `single_emit` case and prints the raw route envelope.

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};

use policy_engine_integration_tests::harness::{self, adapters};

const DEFAULT_SEED: u64 = 0x5C09_EBA1;
const DEFAULT_ITERS: u64 = 64;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    let rest: Vec<String> = args.iter().skip(2).cloned().collect();
    let result = match cmd {
        "fuzz" => cmd_fuzz(&rest),
        "coverage" => cmd_coverage(),
        "replay" => cmd_replay(&rest),
        "corpus" => cmd_corpus(&rest),
        "import-dune" => cmd_import_dune(&rest),
        "-h" | "--help" | "help" | "" => {
            usage();
            return;
        }
        other => {
            eprintln!("unknown subcommand `{other}`\n");
            usage();
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn usage() {
    eprintln!(
        "v3-harness — v3 ActionBody decode harness\n\n\
         USAGE:\n  \
         v3-harness fuzz [--iterations N] [--seed S] [--json PATH]\n  \
         v3-harness coverage\n  \
         v3-harness replay --callkey <chain>__<addr>__<selector> [--seed S]\n  \
         v3-harness corpus [--root DIR]\n  \
         v3-harness import-dune <dune-export.json> [--chain N] [--out PATH]"
    );
}

/// Find `--name VALUE` in args.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn flag_u64(args: &[String], name: &str, default: u64) -> Result<u64> {
    flag(args, name)
        .map(|v| {
            // accept 0x-prefixed hex or decimal
            v.strip_prefix("0x").map_or_else(
                || v.parse::<u64>().map_err(|e| anyhow!("{name}: {e}")),
                |h| u64::from_str_radix(h, 16).map_err(|e| anyhow!("{name}: {e}")),
            )
        })
        .transpose()
        .map(|o| o.unwrap_or(default))
}

fn cmd_fuzz(args: &[String]) -> Result<()> {
    let iters = flag_u64(args, "--iterations", DEFAULT_ITERS)?;
    let seed = flag_u64(args, "--seed", DEFAULT_SEED)?;
    eprintln!("fuzzing all strategies: seed={seed:#x} iterations/callkey={iters}");
    let report = harness::run_synthetic_all(seed, iters)?;
    println!("{}", report.summary());
    if let Some(path) = flag(args, "--json") {
        let json = serde_json::to_string_pretty(&report).context("serialize report")?;
        std::fs::write(path, json).with_context(|| format!("write {path}"))?;
        eprintln!("wrote JSON report to {path}");
    }
    if report.hard_failures() > 0 {
        eprintln!(
            "\n{} HARD FAILURE(S) — see report above",
            report.hard_failures()
        );
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_coverage() -> Result<()> {
    let surface = adapters::load_and_install()?;
    let mut by_strategy: BTreeMap<&str, usize> = BTreeMap::new();
    let mut typed_only = 0usize;
    let mut typed_witness = 0usize;
    for c in &surface.calls {
        *by_strategy.entry(c.strategy.as_str()).or_default() += 1;
    }
    for t in &surface.typed {
        if t.witness_type.is_some() {
            typed_witness += 1;
        } else {
            typed_only += 1;
        }
    }
    println!("Local adapter surface (registryV2/index):");
    println!(
        "  callkeys={}  typed_data_keys={}  unique_bundles={}  install_failures={}",
        surface.total_callkeys,
        surface.total_typed_keys,
        surface.installed_bundle_ids.len(),
        surface.install_failures.len(),
    );
    println!("\nby-callkey strategy breakdown:");
    for (s, n) in &by_strategy {
        println!("  {s:<24} {n}");
    }
    println!("\ntyped-data:");
    println!("  synthesizable (Permit/Hyperliquid)  {typed_only}");
    println!("  witness (UniswapX) → corpus-only     {typed_witness}");
    println!("\ndeferred to corpus replay (not synthetic-fuzzed):");
    println!("  - opcode_stream V4 modifyLiquidities / nested 0x10 / sub-plan 0x21");
    println!("  - typed-data UniswapX witness orders");
    println!("  - native-transfer sentinel 0x00000000");
    Ok(())
}

fn cmd_replay(args: &[String]) -> Result<()> {
    let callkey = flag(args, "--callkey").ok_or_else(|| anyhow!("--callkey required"))?;
    let seed = flag_u64(args, "--seed", DEFAULT_SEED)?;
    let envelope = harness::replay(callkey, seed)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&envelope).context("serialize envelope")?
    );
    Ok(())
}

fn cmd_corpus(args: &[String]) -> Result<()> {
    let root =
        flag(args, "--root").map_or_else(harness::default_corpus_root, std::path::PathBuf::from);
    let outcomes = harness::corpus::run_corpus(&root)?;
    let total = outcomes.len();
    let matched = outcomes.iter().filter(|o| o.matched).count();
    for o in &outcomes {
        let mark = if o.matched { "ok  " } else { "MISS" };
        println!(
            "  {mark} [{}] {} expect={} got={}",
            o.source, o.label, o.expect, o.got
        );
    }
    println!(
        "\ncorpus: {matched}/{total} matched (root {})",
        root.display()
    );
    if matched < total {
        std::process::exit(1);
    }
    Ok(())
}

/// Convert a Dune export (JSON: a bare array of rows, or `{result:{rows:[...]}}`,
/// or `{rows:[...]}`) into the v3 corpus format on stdout (or `--out`). Maps the
/// common Dune column names (`hash`/`tx_hash`, `to`/`to_address`/`contract_address`,
/// `data`/`input`/`calldata`, `value`, `chain_id`). The result needs `expect`
/// annotation by hand (defaulted to `"pass"`).
fn cmd_import_dune(args: &[String]) -> Result<()> {
    let path = args
        .iter()
        .find(|a| !a.starts_with("--") && a.ends_with(".json"))
        .ok_or_else(|| anyhow!("usage: import-dune <dune-export.json> [--chain N] [--out PATH]"))?;
    let default_chain = flag_u64(args, "--chain", 1)?;
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let v: serde_json::Value = serde_json::from_str(&raw).context("parse dune export")?;
    let rows = v
        .as_array()
        .cloned()
        .or_else(|| v.get("rows").and_then(|r| r.as_array()).cloned())
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("rows"))
                .and_then(|r| r.as_array())
                .cloned()
        })
        .ok_or_else(|| {
            anyhow!("no rows array found (expected [...] / {{rows}} / {{result.rows}})")
        })?;

    let pick = |row: &serde_json::Value, keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| row.get(*k).and_then(|x| x.as_str()).map(ToOwned::to_owned))
    };
    let mut txs = Vec::new();
    for row in &rows {
        let Some(to) = pick(row, &["to", "to_address", "contract_address"]) else {
            continue;
        };
        let Some(data) = pick(row, &["data", "input", "calldata"]) else {
            continue;
        };
        let chain = row
            .get("chain_id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(default_chain);
        let value = pick(row, &["value"]).unwrap_or_else(|| "0".to_owned());
        let tx_hash = pick(row, &["hash", "tx_hash"]).unwrap_or_default();
        txs.push(serde_json::json!({
            "expect": "pass",
            "tx_hash": tx_hash,
            "chain_id": chain,
            "rpc": { "params": [{ "to": to, "value": value, "data": data }] }
        }));
    }
    let out = serde_json::to_string_pretty(&serde_json::json!({
        "_comment": format!("imported from {path} via v3-harness import-dune — annotate `expect`/`expect_domain` before committing"),
        "transactions": txs
    }))?;
    if let Some(p) = flag(args, "--out") {
        std::fs::write(p, &out).with_context(|| format!("write {p}"))?;
        eprintln!("wrote {} transactions to {p}", rows.len());
    } else {
        println!("{out}");
    }
    Ok(())
}
