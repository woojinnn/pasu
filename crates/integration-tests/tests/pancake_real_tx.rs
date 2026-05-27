//! Real on-chain **PancakeSwap** transaction harness for the declarative
//! (Tier A) routing path. Mirrors `tests/uniswap_real_tx.rs` structure — only
//! the corpus location, family taxonomy, and per-finding regression guards
//! diverge.
//!
//! Phase 5 (TIER_AB_PLAYBOOK) — exercises:
//!   1. L0~L4 layered verdict for the production routing driver: registry
//!      bundle JSON → `decode_with_json_abi` → `DeclarativeMapper` →
//!      `ActionEnvelope[]` → `policy_request_from_envelope`.
//!   2. DEFECT_CATALOG regression guards D001 (NFPM dual-tokenId collect),
//!      D004 (UR STABLE_SWAP raw decode), D006 (forked Commands.sol opcode
//!      mismatch — Pancake 0x11/0x12 placeholder, 0x13/0x14 INFI INIT).
//!   3. Chain-scope filtering — Phase D scope (Arbitrum / Optimism / Polygon)
//!      transactions must MISS at L0 (no registry callkey for those chains).
//!
//! Corpus: `data/golden/pancake-real-tx/corpus.json` — real BSC / Ethereum /
//! Base PancakeSwap transactions sampled 2026-05-25 (BscScan / Etherscan /
//! BaseScan most-recent verified-contract user txs).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr as _;

use abi_resolver::bridge::{decode_with_json_abi, DecodeWithJsonAbiError};
use abi_resolver::{CallMatchKey, DecodedCall};
use mappers::declarative::{AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{ChildResolver, MapContext, Mapper, MapperError};
use mappers::EmptyTokenRegistry;
use policy_engine::action::{Address, DecimalString};
use policy_engine::{policy_request_from_envelope, ActionEnvelope};

// ───────────────────────────────────────────────────────────────────────────
// Paths — `env!("CARGO_MANIFEST_DIR")` relative, self-contained like
// `uniswap_real_tx.rs`. The registry lives at the worktree root, two levels
// up from `crates/integration-tests/`.
// ───────────────────────────────────────────────────────────────────────────

/// Worktree root.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// `registry/index/by-callkey/`.
fn by_callkey_dir() -> PathBuf {
    workspace_root()
        .join("registry")
        .join("index")
        .join("by-callkey")
}

/// `data/golden/pancake-real-tx/corpus.json`.
fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("golden")
        .join("pancake-real-tx")
        .join("corpus.json")
}

// ───────────────────────────────────────────────────────────────────────────
// Callkey computation — production routing key.
// callkey = `<chain_id>__<to_lowercase>__<selector_lowercase>`
// ───────────────────────────────────────────────────────────────────────────

fn callkey(chain_id: u64, to: &str, selector: &[u8; 4]) -> String {
    format!(
        "{}__{}__0x{}",
        chain_id,
        to.to_ascii_lowercase(),
        hex::encode(selector),
    )
}

#[derive(Debug, serde::Deserialize)]
struct CallKeyIndexEntry {
    #[allow(dead_code)]
    matched: bool,
    bundle_id: String,
    #[allow(dead_code)]
    manifest_path: String,
    #[allow(dead_code)]
    bundle_sha256: String,
    bundle: AdapterFunctionBundle,
}

fn resolve_callkey(
    chain_id: u64,
    to: &str,
    selector: &[u8; 4],
) -> Result<Option<CallKeyIndexEntry>, String> {
    let path = by_callkey_dir().join(format!("{}.json", callkey(chain_id, to, selector)));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let entry: CallKeyIndexEntry =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(entry))
}

// ───────────────────────────────────────────────────────────────────────────
// Mock ChildResolver — multicall_recurse + cross-target dispatch.
// Identical in shape to `uniswap_real_tx.rs::LocalIndexChildResolver`.
// ───────────────────────────────────────────────────────────────────────────

struct LocalIndexChildResolver;

impl ChildResolver for LocalIndexChildResolver {
    fn resolve_child(
        &self,
        child: &CallMatchKey,
        ctx: &MapContext<'_>,
        child_calldata: &[u8],
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let to_str = child.to.to_string();
        let entry = resolve_callkey(child.chain_id, &to_str, &child.selector)
            .map_err(|e| MapperError::Internal(anyhow::anyhow!(e)))?;

        let Some(entry) = entry else {
            return Ok(vec![]);
        };

        let mapper = DeclarativeMapper::new(entry.bundle);
        let abi_json = &mapper.bundle().abi_fragment.abi;
        let mut decoded = decode_with_json_abi(abi_json, child_calldata).map_err(|e| {
            MapperError::Internal(anyhow::anyhow!(
                "child decode failed (callkey {}__{}__0x{}): {e}",
                child.chain_id,
                to_str.to_ascii_lowercase(),
                hex::encode(child.selector),
            ))
        })?;

        decoded.decoder_id = mapper.declarative_decoder_id();
        mapper.map(ctx, &decoded)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Corpus model.
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct Corpus {
    transactions: Vec<CorpusTx>,
}

#[derive(Debug, serde::Deserialize)]
struct CorpusTx {
    family: String,
    intent: String,
    expect: String,
    tx_hash: String,
    chain_id: u64,
    rpc: CorpusRpc,
}

#[derive(Debug, serde::Deserialize)]
struct CorpusRpc {
    #[allow(dead_code)]
    method: String,
    params: Vec<CorpusParam>,
}

#[derive(Debug, serde::Deserialize)]
struct CorpusParam {
    from: String,
    to: String,
    /// Decimal string (wei).
    value: String,
    /// `0x`-prefixed calldata.
    data: String,
}

fn load_corpus() -> Corpus {
    let bytes = fs::read(corpus_path()).expect("corpus.json present");
    serde_json::from_slice(&bytes).expect("corpus.json is valid JSON")
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(stripped).map_err(|e| format!("invalid hex: {e}"))
}

fn selector_of(calldata: &[u8]) -> Result<[u8; 4], String> {
    if calldata.len() < 4 {
        return Err(format!(
            "calldata too short for selector ({} bytes)",
            calldata.len()
        ));
    }
    let mut sel = [0u8; 4];
    sel.copy_from_slice(&calldata[..4]);
    Ok(sel)
}

// ───────────────────────────────────────────────────────────────────────────
// Per-transaction evaluation — L0 / L1 / L2 / L4.
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum StageResult {
    RouteHit {
        bundle_id: String,
        strategy: &'static str,
    },
    RouteMiss,
    Fault(String),
}

fn strategy_label(bundle: &AdapterFunctionBundle) -> &'static str {
    use mappers::declarative::EmitRule;
    match bundle.emit {
        EmitRule::SingleEmit { .. } => "single_emit",
        EmitRule::OpcodeStreamDispatch { .. } => "opcode_stream_dispatch",
        EmitRule::EnumTaggedDispatch { .. } => "enum_tagged_dispatch",
        EmitRule::MulticallRecurse { .. } => "multicall_recurse",
        EmitRule::ArrayEmit { .. } => "array_emit",
    }
}

struct TxVerdict {
    route: StageResult,
    decode: Option<Result<DecodedCall, DecodeWithJsonAbiError>>,
    map: Option<Result<Vec<ActionEnvelope>, MapperError>>,
    lower: Option<(usize, usize)>,
}

impl TxVerdict {
    fn fully_passed(&self) -> bool {
        matches!(self.route, StageResult::RouteHit { .. })
            && matches!(self.decode, Some(Ok(_)))
            && matches!(&self.map, Some(Ok(envs)) if !envs.is_empty())
            && matches!(self.lower, Some((some, total)) if some == total && total > 0)
    }

    fn partial(&self) -> bool {
        matches!(self.route, StageResult::RouteHit { .. }) && !self.fully_passed()
    }

    fn missed(&self) -> bool {
        matches!(self.route, StageResult::RouteMiss)
    }
}

/// Evaluate L0~L4 for one transaction. Never panics.
fn evaluate(tx: &CorpusTx) -> TxVerdict {
    let param = match tx.rpc.params.first() {
        Some(p) => p,
        None => {
            return TxVerdict {
                route: StageResult::Fault("rpc.params empty".into()),
                decode: None,
                map: None,
                lower: None,
            };
        }
    };

    let calldata = match decode_hex(&param.data) {
        Ok(c) => c,
        Err(e) => {
            return TxVerdict {
                route: StageResult::Fault(format!("calldata {e}")),
                decode: None,
                map: None,
                lower: None,
            };
        }
    };
    let selector = match selector_of(&calldata) {
        Ok(s) => s,
        Err(e) => {
            return TxVerdict {
                route: StageResult::Fault(e),
                decode: None,
                map: None,
                lower: None,
            };
        }
    };

    // ── L0 — route via byCallKey index ─────────────────────────────────────
    let entry = match resolve_callkey(tx.chain_id, &param.to, &selector) {
        Ok(Some(e)) => e,
        Ok(None) => {
            return TxVerdict {
                route: StageResult::RouteMiss,
                decode: None,
                map: None,
                lower: None,
            };
        }
        Err(e) => {
            return TxVerdict {
                route: StageResult::Fault(e),
                decode: None,
                map: None,
                lower: None,
            };
        }
    };
    let strategy = strategy_label(&entry.bundle);
    let route = StageResult::RouteHit {
        bundle_id: entry.bundle_id.clone(),
        strategy,
    };

    let mapper = DeclarativeMapper::new(entry.bundle);

    // ── L1 — decode calldata against the bundle ABI ────────────────────────
    let abi_json = &mapper.bundle().abi_fragment.abi;
    let decode_result = decode_with_json_abi(abi_json, &calldata);
    let mut decoded = match &decode_result {
        Ok(d) => {
            let mut d = d.clone();
            d.decoder_id = mapper.declarative_decoder_id();
            d
        }
        Err(_) => {
            return TxVerdict {
                route,
                decode: Some(decode_result),
                map: None,
                lower: None,
            };
        }
    };
    decoded.decoder_id = mapper.declarative_decoder_id();

    // ── L2 — map decoded call to ActionEnvelope[] ──────────────────────────
    let from = match Address::from_str(&param.from) {
        Ok(a) => a,
        Err(e) => {
            return TxVerdict {
                route,
                decode: Some(decode_result),
                map: Some(Err(MapperError::Internal(anyhow::anyhow!(
                    "invalid tx.from {:?}: {e}",
                    param.from
                )))),
                lower: None,
            };
        }
    };
    let to = match Address::from_str(&param.to) {
        Ok(a) => a,
        Err(e) => {
            return TxVerdict {
                route,
                decode: Some(decode_result),
                map: Some(Err(MapperError::Internal(anyhow::anyhow!(
                    "invalid tx.to {:?}: {e}",
                    param.to
                )))),
                lower: None,
            };
        }
    };
    let value = match DecimalString::from_str(&param.value) {
        Ok(v) => v,
        Err(e) => {
            return TxVerdict {
                route,
                decode: Some(decode_result),
                map: Some(Err(MapperError::Internal(anyhow::anyhow!(
                    "invalid tx.value {:?}: {e}",
                    param.value
                )))),
                lower: None,
            };
        }
    };

    let registry = EmptyTokenRegistry;
    let resolver = LocalIndexChildResolver;
    let ctx = MapContext {
        chain_id: tx.chain_id,
        from: &from,
        to: &to,
        value_wei: &value,
        block_timestamp: None,
        token_registry: &registry,
        parent_calldata: None,
        depth: 0,
        resolver: Some(&resolver),
    };

    let map_result = mapper.map(&ctx, &decoded);
    let envelopes = match &map_result {
        Ok(envs) => envs.clone(),
        Err(_) => {
            return TxVerdict {
                route,
                decode: Some(decode_result),
                map: Some(map_result),
                lower: None,
            };
        }
    };

    // ── L4 — lower each envelope to a PolicyRequest ────────────────────────
    let total = envelopes.len();
    let some_count = envelopes
        .iter()
        .filter(|env| {
            policy_request_from_envelope(env, &from, &to, &value, tx.chain_id, 0).is_some()
        })
        .count();

    TxVerdict {
        route,
        decode: Some(decode_result),
        map: Some(map_result),
        lower: Some((some_count, total)),
    }
}

fn envelopes_json(verdict: &TxVerdict) -> String {
    match &verdict.map {
        Some(Ok(envs)) => {
            serde_json::to_string(envs).unwrap_or_else(|e| format!("<serialize error: {e}>"))
        }
        Some(Err(_)) | None => "<n/a>".to_string(),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Test A — harness_self_check (strict).
// ───────────────────────────────────────────────────────────────────────────

/// Unit-verifies the harness components before the corpus is trusted.
#[test]
fn harness_self_check() {
    // (1) callkey computation matches a known PancakeSwap V2 Router (BSC) key.
    let bsc_v2_router = "0x10ED43C718714eb63d5aA57B78B54704E256024E";
    let v2_swap_selector = [0x38, 0xed, 0x17, 0x39]; // swapExactTokensForTokens
    let key = callkey(56, bsc_v2_router, &v2_swap_selector);
    assert_eq!(
        key, "56__0x10ed43c718714eb63d5aa57b78b54704e256024e__0x38ed1739",
        "callkey format / lowercasing regression",
    );

    // (2) the byCallKey index dir exists and the Pancake BSC V2 swap callkey
    //     loads + parses into a complete bundle.
    assert!(
        by_callkey_dir().is_dir(),
        "byCallKey index dir missing: {}",
        by_callkey_dir().display(),
    );
    let entry = resolve_callkey(56, bsc_v2_router, &v2_swap_selector)
        .expect("Pancake BSC V2 swap callkey must parse")
        .expect("Pancake BSC V2 swap callkey must exist");
    assert_eq!(
        entry.bundle.match_.selector.to_ascii_lowercase(),
        "0x38ed1739"
    );
    assert_eq!(strategy_label(&entry.bundle), "single_emit");
    assert!(
        entry.bundle_id.starts_with("pancake/v2-router/"),
        "expected pancake/v2-router/* bundle, got {}",
        entry.bundle_id,
    );

    // (3) corpus.json parses and has ≥ 20 entries (the plan-mandated minimum).
    let corpus = load_corpus();
    assert!(
        corpus.transactions.len() >= 20,
        "corpus.json expected ≥ 20 transactions, got {}",
        corpus.transactions.len(),
    );

    // (4) the first BSC `v2_swap` corpus entry passes L0~L2 + L4.
    let v2_tx = corpus
        .transactions
        .iter()
        .find(|t| t.family == "v2_swap" && t.chain_id == 56)
        .expect("corpus must contain a BSC v2_swap transaction");
    let verdict = evaluate(v2_tx);
    assert!(
        matches!(verdict.route, StageResult::RouteHit { .. }),
        "v2_swap self-check tx {} — L0 route expected HIT, got {:?}",
        v2_tx.tx_hash,
        verdict.route,
    );
    assert!(
        matches!(verdict.decode, Some(Ok(_))),
        "v2_swap self-check tx {} — L1 decode expected OK, got {:?}",
        v2_tx.tx_hash,
        verdict.decode.as_ref().map(|r| r.as_ref().err()),
    );
    match &verdict.map {
        Some(Ok(envs)) => assert!(
            !envs.is_empty(),
            "v2_swap self-check tx {} — L2 expected ≥1 envelope",
            v2_tx.tx_hash,
        ),
        other => panic!(
            "v2_swap self-check tx {} — L2 map expected Ok, got {other:?}",
            v2_tx.tx_hash,
        ),
    }
    match verdict.lower {
        Some((some, total)) => assert!(
            some == total && total > 0,
            "v2_swap self-check tx {} — L4 expected all {total} envelopes -> Some, got {some}",
            v2_tx.tx_hash,
        ),
        None => panic!(
            "v2_swap self-check tx {} — L4 lowering not reached",
            v2_tx.tx_hash,
        ),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Test B — corpus_verification (lenient — prints verdict table).
// ───────────────────────────────────────────────────────────────────────────

/// Walk every corpus transaction, evaluate L0~L4, print a verdict table.
///
/// Run with `cargo test -p integration-tests --test pancake_real_tx -- --nocapture`.
///
/// Assertion policy:
///   * `expect == "pass"`           → must route + decode + map + lower (L0~L4)
///   * `expect == "excluded"`       → must MISS at L0 (Phase D scope chains)
///   * `expect == "known_defect_*"` → recorded under that defect tag; not asserted
///     either way (the per-defect entry in DEFECT_CATALOG governs the eventual
///     fix). Per-tx verdict table is still printed for human L3 review.
#[test]
fn corpus_verification() {
    let corpus = load_corpus();
    let mut processed = 0usize;

    // family → (full pass, partial, miss) tallies.
    let mut family_summary: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    let mut pass_failures: Vec<String> = Vec::new();
    let mut excluded_outcomes: Vec<String> = Vec::new();
    let mut excluded_unexpected: Vec<String> = Vec::new();
    // `known_defect_*` outcomes — surfaced as warnings, not failures.
    let mut known_defect_outcomes: BTreeMap<String, Vec<String>> = BTreeMap::new();

    println!();
    println!("════════════════════════════════════════════════════════════════════════");
    println!(
        " pancake_real_tx — declarative (Tier A) corpus verification — {} tx",
        corpus.transactions.len()
    );
    println!("════════════════════════════════════════════════════════════════════════");

    for tx in &corpus.transactions {
        let verdict = evaluate(tx);
        processed += 1;

        let short_hash = {
            let h = tx.tx_hash.trim_start_matches("0x");
            let take = h.len().min(8);
            format!("0x{}...", &h[..take])
        };
        println!();
        println!(
            "[{}/{}] {} (chain {}, expect={})",
            tx.family, tx.intent, short_hash, tx.chain_id, tx.expect,
        );

        match &verdict.route {
            StageResult::RouteHit {
                bundle_id,
                strategy,
            } => {
                println!("  L0 route : HIT  (bundle {bundle_id}, strategy {strategy})");
            }
            StageResult::RouteMiss => {
                println!("  L0 route : MISS");
            }
            StageResult::Fault(e) => {
                println!("  L0 route : FAULT ({e})");
            }
        }

        match &verdict.decode {
            Some(Ok(_)) => println!("  L1 decode: OK"),
            Some(Err(e)) => println!("  L1 decode: FAIL ({e})"),
            None => {
                if matches!(verdict.route, StageResult::RouteMiss) {
                    println!("  L1 decode: -- (route miss)");
                } else {
                    println!("  L1 decode: -- (not reached)");
                }
            }
        }

        match &verdict.map {
            Some(Ok(envs)) => {
                if envs.is_empty() {
                    println!("  L2 map   : 0 envelopes");
                } else {
                    println!("  L2 map   : {} envelope(s)", envs.len());
                }
            }
            Some(Err(e)) => println!("  L2 map   : MapperError: {e}"),
            None => println!("  L2 map   : -- (not reached)"),
        }

        match verdict.lower {
            Some((some, total)) => {
                if total == 0 {
                    println!("  L4 lower : -- (0 envelopes)");
                } else if some == total {
                    println!("  L4 lower : OK ({some}/{total} envelopes -> Some)");
                } else {
                    println!("  L4 lower : FAIL ({some}/{total} -> Some)");
                }
            }
            None => println!("  L4 lower : -- (not reached)"),
        }

        println!("  envelopes: {}", envelopes_json(&verdict));

        let slot = family_summary.entry(tx.family.clone()).or_insert([0, 0, 0]);
        if verdict.fully_passed() {
            slot[0] += 1;
        } else if verdict.partial() {
            slot[1] += 1;
        } else if verdict.missed() {
            slot[2] += 1;
        } else {
            slot[2] += 1;
        }

        if tx.expect == "pass" && !verdict.fully_passed() {
            let stage = first_failed_stage(&verdict);
            pass_failures.push(format!(
                "  - [{}/{}] {} — failed at {}",
                tx.family, tx.intent, short_hash, stage,
            ));
        }
        if tx.expect == "excluded" {
            let outcome = if verdict.missed() {
                "MISS (correct — intentionally out of scope)".to_string()
            } else if verdict.fully_passed() {
                "FULLY PASSED (unexpected — excluded fn produced a verdict)".to_string()
            } else {
                format!("PARTIAL ({})", first_failed_stage(&verdict))
            };
            let line = format!(
                "  - [{}/{}] {} — {}",
                tx.family, tx.intent, short_hash, outcome,
            );
            if !verdict.missed() {
                excluded_unexpected.push(line.clone());
            }
            excluded_outcomes.push(line);
        }
        if tx.expect.starts_with("known_defect_") {
            let stage = if verdict.fully_passed() {
                "(unexpectedly fully passed — defect may be fixed)".to_string()
            } else {
                first_failed_stage(&verdict).to_string()
            };
            let line = format!(
                "  - [{}/{}] {} — observed: {}",
                tx.family, tx.intent, short_hash, stage,
            );
            known_defect_outcomes
                .entry(tx.expect.clone())
                .or_default()
                .push(line);
        }
    }

    println!();
    println!("════════════════════════════════════════════════════════════════════════");
    println!(" Family summary — full pass / partial / miss");
    println!("════════════════════════════════════════════════════════════════════════");
    for (family, [pass, partial, miss]) in &family_summary {
        println!("  {family:<18}  pass={pass:<3} partial={partial:<3} miss={miss}");
    }
    let total_pass: usize = family_summary.values().map(|s| s[0]).sum();
    let total_partial: usize = family_summary.values().map(|s| s[1]).sum();
    let total_miss: usize = family_summary.values().map(|s| s[2]).sum();
    println!(
        "  {:<18}  pass={total_pass:<3} partial={total_partial:<3} miss={total_miss}",
        "TOTAL",
    );

    println!();
    println!("────────────────────────────────────────────────────────────────────────");
    if pass_failures.is_empty() {
        println!(" expect==\"pass\" transactions that did not fully pass: NONE");
    } else {
        println!(
            " expect==\"pass\" transactions that did NOT fully pass ({}):",
            pass_failures.len()
        );
        for line in &pass_failures {
            println!("{line}");
        }
    }

    println!();
    println!("────────────────────────────────────────────────────────────────────────");
    println!(
        " expect==\"excluded\" transactions ({}):",
        excluded_outcomes.len()
    );
    for line in &excluded_outcomes {
        println!("{line}");
    }

    if !known_defect_outcomes.is_empty() {
        println!();
        println!("────────────────────────────────────────────────────────────────────────");
        let total: usize = known_defect_outcomes.values().map(Vec::len).sum();
        println!(
            " expect==\"known_defect_*\" transactions ({}, D007~D009 candidates):",
            total
        );
        for (tag, lines) in &known_defect_outcomes {
            println!("  [{tag}]");
            for line in lines {
                println!("  {line}");
            }
        }
    }

    println!("════════════════════════════════════════════════════════════════════════");
    println!();

    assert_eq!(
        processed,
        corpus.transactions.len(),
        "harness must process every corpus transaction without panicking",
    );
    assert!(
        pass_failures.is_empty(),
        "declarative (Tier A) regression — expect==\"pass\" tx did not fully pass L0~L4:\n{}",
        pass_failures.join("\n"),
    );
    assert!(
        excluded_unexpected.is_empty(),
        "expect==\"excluded\" tx unexpectedly produced a declarative verdict:\n{}",
        excluded_unexpected.join("\n"),
    );
}

fn first_failed_stage(verdict: &TxVerdict) -> &'static str {
    match &verdict.route {
        StageResult::RouteMiss => return "L0 route (MISS)",
        StageResult::Fault(_) => return "L0 route (FAULT)",
        StageResult::RouteHit { .. } => {}
    }
    match &verdict.decode {
        Some(Err(_)) => return "L1 decode",
        None => return "L1 decode (not reached)",
        Some(Ok(_)) => {}
    }
    match &verdict.map {
        Some(Err(_)) => return "L2 map (MapperError)",
        Some(Ok(envs)) if envs.is_empty() => return "L2 map (0 envelopes)",
        None => return "L2 map (not reached)",
        Some(Ok(_)) => {}
    }
    match verdict.lower {
        Some((some, total)) if some != total || total == 0 => "L4 lower",
        None => "L4 lower (not reached)",
        Some(_) => "none",
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Test C — defect_catalog_regression (strict).
//
// Per-D-entry regression guard. `corpus_verification` covers L0/L1/L2/L4
// shape; this test locks the per-defect L3 envelope corrections so a future
// change that re-introduces a known bug fails CI here.
// ───────────────────────────────────────────────────────────────────────────

/// Find the first corpus tx whose `(family, intent_needle)` matches.
fn find_tx<'a>(corpus: &'a Corpus, family: &str, intent_needle: &str) -> Option<&'a CorpusTx> {
    corpus
        .transactions
        .iter()
        .find(|t| t.family == family && t.intent.contains(intent_needle))
}

/// Evaluate the tx and unwrap its envelopes as a JSON array. Panics if the
/// tx is not present or any L0~L4 stage fails — the caller is asserting the
/// envelope shape, not testing the pipeline.
fn envelopes_of_tx(corpus: &Corpus, family: &str, intent_needle: &str) -> serde_json::Value {
    let tx = find_tx(corpus, family, intent_needle).unwrap_or_else(|| {
        panic!("corpus: no tx with family={family} + intent containing {intent_needle:?}")
    });
    let verdict = evaluate(tx);
    match verdict.map {
        Some(Ok(envelopes)) => serde_json::to_value(&envelopes).expect("envelopes serialise"),
        other => panic!(
            "tx {} ({family}/{intent_needle}) — L2 map did not produce envelopes ({other:?})",
            tx.tx_hash
        ),
    }
}

#[test]
fn defect_catalog_regression() {
    let corpus = load_corpus();

    // ── D001 — NFPM `collect` dual-tokenId: envelopes must surface a non-empty
    //          tokenId on the nft AssetRef (kind=erc721 invariant). ─────────
    //
    // The standalone V3 NFPM collect tx targets `0x46A15B0b...` selector
    // `0xfc6f7865` (`collect((uint256,address,uint128,uint128))`). The
    // mapper's `build_claim_rewards_envelope` should inject the root tokenId
    // into the nft AssetRef so the policy-engine deserialize invariant
    // (`kind=erc721 → tokenId required`) holds.
    let collect_envs = envelopes_of_tx(&corpus, "v3_lp", "collect");
    let nft = collect_envs
        .as_array()
        .and_then(|a| a.first())
        .and_then(|e| e.pointer("/fields/nft"))
        .unwrap_or_else(|| panic!("D001: v3_lp/collect envelope missing fields.nft"));
    let nft_kind = nft.get("kind").and_then(serde_json::Value::as_str);
    assert!(
        matches!(nft_kind, Some("erc721") | Some("erc1155")),
        "D001 regression — v3_lp/collect: fields.nft.kind expected erc721/erc1155, got {nft_kind:?}",
    );
    let nft_token_id = nft.get("tokenId").and_then(serde_json::Value::as_str);
    assert!(
        matches!(nft_token_id, Some(s) if !s.is_empty()),
        "D001 regression — v3_lp/collect: fields.nft.tokenId must be non-empty (kind=erc721 invariant); envelope={collect_envs}",
    );

    // ── D006 — UR `execute` callkey routes via `opcode_stream_dispatch` with
    //          the Pancake-specific dispatcher_id. ──────────────────────────
    //
    // Phase 4 corrected `pancake_ur.rs`: 0x11/0x12 placeholder (omit entry),
    // 0x13/0x14 INFI_CL/BIN_INITIALIZE_POOL with PoolKey 6-field ABI,
    // 0x22/0x23 STABLE_SWAP with `(address[] path, uint256[] flag)`. The UR
    // entry callkey must hit a bundle whose strategy is
    // `opcode_stream_dispatch` and whose bundle_id starts with
    // `pancake/universal-router/`. Inner-opcode envelope emission depends on
    // the manifest's per_opcode_emit coverage (see D008 candidate) and is not
    // asserted here.
    let ur_tx = corpus
        .transactions
        .iter()
        .find(|t| t.family == "ur_execute")
        .expect("D006: corpus must include at least one ur_execute tx");
    let ur_verdict = evaluate(ur_tx);
    let StageResult::RouteHit {
        ref bundle_id,
        strategy,
    } = ur_verdict.route
    else {
        panic!(
            "D006 regression — UR execute tx {} L0 route expected HIT, got {:?}",
            ur_tx.tx_hash, ur_verdict.route
        );
    };
    assert!(
        bundle_id.starts_with("pancake/universal-router/"),
        "D006 regression — UR execute tx {} routed to wrong bundle: {bundle_id}",
        ur_tx.tx_hash,
    );
    assert_eq!(
        strategy, "opcode_stream_dispatch",
        "D006 regression — UR execute tx {} expected opcode_stream_dispatch strategy",
        ur_tx.tx_hash,
    );
    // L1 decode must succeed for a correctly-shaped UR `execute` calldata —
    // the manifest's `execute(bytes,bytes[],uint256)` ABI matches the
    // on-chain function signature exactly.
    assert!(
        matches!(ur_verdict.decode, Some(Ok(_))),
        "D006 regression — UR execute tx {} L1 decode expected OK, got {:?}",
        ur_tx.tx_hash,
        ur_verdict.decode.as_ref().map(|r| r.as_ref().err()),
    );

    // ── Permit2 — selector `0x87517c45` (NOT the report's draft `0x6dae5937`)
    //          routes via the correct chain_to_addresses scope. ──────────────
    let permit2_tx = corpus
        .transactions
        .iter()
        .find(|t| t.family == "permit2")
        .expect("corpus must include a permit2 tx");
    let permit2_verdict = evaluate(permit2_tx);
    let StageResult::RouteHit {
        bundle_id: ref permit2_bundle,
        ..
    } = permit2_verdict.route
    else {
        panic!(
            "Permit2 tx {} L0 route expected HIT, got {:?}",
            permit2_tx.tx_hash, permit2_verdict.route
        );
    };
    assert!(
        permit2_bundle.starts_with("pancake/permit2/approve@"),
        "Permit2 tx {} routed to wrong bundle: {permit2_bundle}",
        permit2_tx.tx_hash,
    );
    assert!(
        permit2_verdict.fully_passed(),
        "Permit2 tx {} did not fully pass L0~L4 (stage: {})",
        permit2_tx.tx_hash,
        first_failed_stage(&permit2_verdict),
    );

    // ── Chain-scope filter — Phase D scope tx (Arbitrum 42161 / Optimism 10 /
    //          Polygon 137 etc) MUST MISS at L0. Registry has no chain_to_addresses
    //          entry for those chains so the callkey file simply does not exist. ─
    for tx in &corpus.transactions {
        if tx.expect == "excluded" {
            let verdict = evaluate(tx);
            assert!(
                verdict.missed(),
                "Chain-scope filter regression — excluded tx {} (chain {}) did not MISS at L0",
                tx.tx_hash,
                tx.chain_id,
            );
        }
    }
}
