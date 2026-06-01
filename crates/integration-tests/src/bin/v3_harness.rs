//! `v3-harness` — CLI for the v3 `ActionBody[]` decode harness.
//!
//! Drives the production decode entrypoints over the local `registryV2/index`
//! surface — no browser, no WASM runtime, no RPC. Subcommands:
//!
//! ```text
//! v3-harness fuzz       [--iterations N] [--seed S] [--filter <substr>] [--json PATH]
//! v3-harness validate   [--filter <substr>] [--iterations N]
//! v3-harness coverage
//! v3-harness replay     --callkey <chain>__<addr>__<selector> [--seed S]
//! v3-harness corpus     [--root DIR] [--filter <substr>] [--require-expect-body] [--json PATH]
//! v3-harness import-dune|import-etherscan|import <export.json> [--chain N] [--out PATH]
//! ```
//!
//! `fuzz` runs the all-strategy synthetic sweep and prints the report (optionally
//! dumping the full JSON for CI). `validate` is the **author-time** focused
//! variant: it synthesizes type-valid inputs for the `--filter`-matched
//! `single_emit` manifests, routes each through the production decoder, and
//! fails loud (exit 1) with the exact bundle id + error for any `emit.body` that
//! does not match the typed `ActionBody` struct — catching the missing/renamed
//! field class (e.g. `missing field live_inputs`) at author time instead of
//! decode-test time. `coverage` prints the routable surface broken
//! down by strategy + the categories deferred to corpus replay. `replay`
//! reproduces a single `single_emit` case and prints the raw route envelope.
//! `corpus` replays the committed real-tx corpus; use `--filter <protocol>` for
//! protocol-scoped landing gates and `--require-expect-body` to require
//! field-level semantic assertions on every selected `expect:"pass"` entry.
//! `import-*` convert a Dune or Etherscan export into the corpus JSON format
//! (parse-only, no network).

use std::collections::BTreeMap;

use alloy_primitives::U256;
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
        "validate" => cmd_validate(&rest),
        "coverage" => cmd_coverage(),
        "replay" => cmd_replay(&rest),
        "corpus" => cmd_corpus(&rest),
        "import-dune" | "import-etherscan" | "import" => cmd_import(&rest),
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
         v3-harness fuzz [--iterations N] [--seed S] [--filter <substr>] [--json PATH]\n  \
         v3-harness validate [--filter <substr>] [--iterations N]\n  \
         v3-harness coverage\n  \
         v3-harness replay --callkey <chain>__<addr>__<selector> [--seed S]\n  \
         v3-harness corpus [--root DIR] [--filter <substr>] [--require-expect-body] [--json PATH]\n  \
         v3-harness import-dune|import-etherscan|import <export.json> [--chain N] [--out PATH]"
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
        .map(|v| parse_u64_quantity(v).map_err(|e| anyhow!("{name}: {e}")))
        .transpose()
        .map(|o| o.unwrap_or(default))
}

fn parse_u64_quantity(value: &str) -> Result<u64> {
    let value = value.trim();
    if value.is_empty() {
        return Err(anyhow!("empty quantity"));
    }
    if let Some(hex) = strip_hex_prefix(value) {
        let hex = if hex.is_empty() { "0" } else { hex };
        return u64::from_str_radix(hex, 16).map_err(|e| anyhow!("invalid hex `{value}`: {e}"));
    }
    value
        .parse::<u64>()
        .map_err(|e| anyhow!("invalid decimal `{value}`: {e}"))
}

fn parse_json_u64_quantity(value: &serde_json::Value) -> Result<u64> {
    match value {
        serde_json::Value::Number(n) => n.as_u64().ok_or_else(|| anyhow!("invalid u64 `{n}`")),
        serde_json::Value::String(s) => parse_u64_quantity(s),
        other => Err(anyhow!("invalid u64 quantity type: {other}")),
    }
}

fn normalize_quantity_string(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok("0".to_owned());
    }
    Ok(parse_u256_quantity(value)?.to_string())
}

fn parse_u256_quantity(value: &str) -> Result<U256> {
    let value = value.trim();
    if value.is_empty() {
        return Err(anyhow!("empty quantity"));
    }
    if let Some(hex) = strip_hex_prefix(value) {
        let hex = if hex.is_empty() { "0" } else { hex };
        return U256::from_str_radix(hex, 16)
            .map_err(|e| anyhow!("invalid U256 hex `{value}`: {e}"));
    }
    if !value.chars().all(|c| c.is_ascii_digit()) {
        return Err(anyhow!("invalid U256 decimal `{value}`"));
    }
    U256::from_str_radix(value, 10).map_err(|e| anyhow!("invalid U256 decimal `{value}`: {e}"))
}

fn strip_hex_prefix(value: &str) -> Option<&str> {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
}

fn cmd_fuzz(args: &[String]) -> Result<()> {
    let iters = flag_u64(args, "--iterations", DEFAULT_ITERS)?;
    let seed = flag_u64(args, "--seed", DEFAULT_SEED)?;
    let filter = flag(args, "--filter");
    let scope = filter.unwrap_or("all");
    eprintln!("fuzzing all strategies: seed={seed:#x} iterations/callkey={iters} filter={scope}");
    let report = harness::run_synthetic_all_filtered(seed, iters, filter)?;
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
    if filter.is_some() && report.total == 0 {
        eprintln!("error: fuzz filter `{scope}` matched no routable entries");
        std::process::exit(1);
    }
    Ok(())
}

/// Author-time `emit.body` shape validator. Synthesizes type-valid inputs for the
/// `--filter`-matched `single_emit` manifests, routes each through the production
/// decoder, and exits 1 (with the exact bundle id + error + repro) if any
/// `emit.body` fails to build a well-formed `ActionBody`. Reads the built
/// `registryV2/index/` — run `npm run build` first (the `check:manifest` npm
/// script chains both).
fn cmd_validate(args: &[String]) -> Result<()> {
    let filter = flag(args, "--filter");
    let iters = flag_u64(args, "--iterations", 24)?;
    let scope = filter.map_or_else(|| "all".to_owned(), |f| format!("filter=`{f}`"));
    let verdicts = harness::validate(filter, iters)?;
    let checked = verdicts.len();
    let failed: Vec<&harness::ManifestVerdict> = verdicts.iter().filter(|v| !v.ok).collect();

    if failed.is_empty() {
        println!("validate ({scope}): {checked} single_emit manifest(s) OK, 0 structural errors  [iters/manifest={iters}]");
        if checked == 0 {
            eprintln!(
                "warning: no single_emit manifests matched. Did you `npm run build` after authoring? Is `--filter` right?"
            );
        }
        return Ok(());
    }

    eprintln!(
        "validate ({scope}): {} of {checked} manifest(s) FAILED emit.body decode:\n",
        failed.len()
    );
    for v in &failed {
        eprintln!("  \u{2717} {}", v.bundle_id);
        eprintln!("      callkey: {}", v.callkey);
        if let Some(e) = &v.error {
            eprintln!("      error:   {e}");
        }
        if let Some(s) = v.seed {
            eprintln!(
                "      repro:   v3-harness replay --callkey {} --seed {s:#x}",
                v.callkey
            );
        }
    }
    eprintln!(
        "\n{} structural error(s). Align emit.body with the typed ActionBody struct, then re-run.",
        failed.len()
    );
    std::process::exit(1);
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
    let source_filter = flag(args, "--filter");
    let require_expect_body = args.iter().any(|a| a == "--require-expect-body");
    let outcomes = harness::corpus::run_corpus_filtered(&root, source_filter)?;
    if let Some(path) = flag(args, "--json") {
        let rows: Vec<serde_json::Value> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "source": o.source,
                    "label": o.label,
                    "expect": o.expect,
                    "got": o.got,
                    "matched": o.matched,
                    "expect_body_assertions": o.expect_body_assertions,
                    "envelope": o.envelope,
                })
            })
            .collect();
        let json = serde_json::to_string_pretty(&rows).context("serialize corpus outcomes")?;
        std::fs::write(path, json).with_context(|| format!("write {path}"))?;
        eprintln!("wrote JSON corpus outcomes to {path}");
    }
    let total = outcomes.len();
    let matched = outcomes.iter().filter(|o| o.matched).count();
    let pass_total = outcomes.iter().filter(|o| o.expect == "pass").count();
    let pass_with_expect_body = outcomes
        .iter()
        .filter(|o| o.expect == "pass" && o.expect_body_assertions > 0)
        .count();
    for o in &outcomes {
        let mark = if o.matched { "ok  " } else { "MISS" };
        println!(
            "  {mark} [{}] {} expect={} got={}",
            o.source, o.label, o.expect, o.got
        );
    }
    let scope = source_filter.unwrap_or("all");
    println!(
        "\ncorpus: {matched}/{total} matched (root {}, filter {scope})",
        root.display(),
    );
    println!("semantic expect_body: {pass_with_expect_body}/{pass_total} pass entries pinned");
    if source_filter.is_some() && total == 0 {
        eprintln!("error: corpus filter `{scope}` matched no entries");
        std::process::exit(1);
    }
    if matched < total {
        std::process::exit(1);
    }
    if require_expect_body && pass_with_expect_body < pass_total {
        eprintln!(
            "error: --require-expect-body failed: {} pass entries lack field-level semantic assertions",
            pass_total - pass_with_expect_body
        );
        for o in outcomes
            .iter()
            .filter(|o| o.expect == "pass" && o.expect_body_assertions == 0)
        {
            eprintln!("  missing expect_body [{}] {}", o.source, o.label);
        }
        std::process::exit(1);
    }
    Ok(())
}

/// Convert a **Dune or Etherscan** export into the v3 corpus format on stdout
/// (or `--out`). Parse-only — no network. Accepts these JSON shapes:
/// * bare array `[...]`
/// * Dune: `{rows:[...]}` or `{result:{rows:[...]}}`
/// * Etherscan `txlist`: `{status,message,result:[...]}`
/// * Etherscan `eth_getTransactionByHash`: `{result:{...}}` (single tx)
///
/// Maps the column names shared by both sources (`hash`/`tx_hash`,
/// `to`/`to_address`/`contract_address`, `data`/`input`/`calldata`, `value`,
/// `chain_id`). Hex RPC quantities are normalized to decimal corpus strings.
/// The result needs `expect`/`expect_domain` annotation by hand (defaulted to
/// `"pass"`). Backs the `import-dune`/`import-etherscan`/`import` subcommands
/// (identical conversion — the source only differs in wrapper shape).
fn cmd_import(args: &[String]) -> Result<()> {
    let path = args
        .iter()
        .find(|a| !a.starts_with("--") && a.ends_with(".json"))
        .ok_or_else(|| {
            anyhow!("usage: import[-dune|-etherscan] <export.json> [--chain N] [--out PATH]")
        })?;
    let default_chain = flag_u64(args, "--chain", 1)?;
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let v: serde_json::Value = serde_json::from_str(&raw).context("parse export")?;
    let rows = v
        .as_array()
        .cloned()
        // Dune: {rows:[...]} / {result:{rows:[...]}}
        .or_else(|| v.get("rows").and_then(|r| r.as_array()).cloned())
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("rows"))
                .and_then(|r| r.as_array())
                .cloned()
        })
        // Etherscan txlist: {status,message,result:[...]}
        .or_else(|| v.get("result").and_then(|r| r.as_array()).cloned())
        // Etherscan eth_getTransactionByHash: {result:{...}} (single tx)
        .or_else(|| {
            v.get("result")
                .filter(|r| r.is_object())
                .map(|o| vec![o.clone()])
        })
        .ok_or_else(|| {
            anyhow!(
                "no rows found (expected [...] / {{rows}} / {{result:[...]}} / {{result:{{...}}}})"
            )
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
        let tx_hash = pick(row, &["hash", "tx_hash"]).unwrap_or_default();
        let chain = row
            .get("chain_id")
            .map(parse_json_u64_quantity)
            .transpose()
            .with_context(|| format!("normalize chain_id for {}", tx_hash_or_unknown(&tx_hash)))?
            .unwrap_or(default_chain);
        let value_raw = pick(row, &["value"]).unwrap_or_else(|| "0".to_owned());
        let value = normalize_quantity_string(&value_raw)
            .with_context(|| format!("normalize value for {}", tx_hash_or_unknown(&tx_hash)))?;
        txs.push(serde_json::json!({
            "expect": "pass",
            "tx_hash": tx_hash,
            "chain_id": chain,
            "rpc": { "params": [{ "to": to, "value": value, "data": data }] }
        }));
    }
    let out = serde_json::to_string_pretty(&serde_json::json!({
        "_comment": format!("imported from {path} via v3-harness import — annotate `expect`/`expect_domain` before committing"),
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

fn tx_hash_or_unknown(tx_hash: &str) -> &str {
    if tx_hash.is_empty() {
        "<unknown>"
    } else {
        tx_hash
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_quantity_string, parse_json_u64_quantity, parse_u64_quantity};

    #[test]
    fn normalizes_hex_value_to_decimal() {
        assert_eq!(
            normalize_quantity_string("0x5af3107a4000").unwrap(),
            "100000000000000"
        );
    }

    #[test]
    fn normalizes_decimal_leading_zeroes() {
        assert_eq!(normalize_quantity_string("00042").unwrap(), "42");
    }

    #[test]
    fn parses_hex_chain_id() {
        assert_eq!(parse_u64_quantity("0x2105").unwrap(), 8453);
        assert_eq!(
            parse_json_u64_quantity(&serde_json::json!("0x2105")).unwrap(),
            8453
        );
    }
}
