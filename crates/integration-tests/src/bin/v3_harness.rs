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
         v3-harness replay --callkey <chain>__<addr>__<selector> [--seed S]"
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
