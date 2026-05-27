//! Real on-chain Uniswap transaction harness for the **declarative (Tier A)**
//! routing path.
//!
//! `golden_regression.rs` exercises the *static* mapper pipeline. This harness
//! instead reproduces the production verdict driver: registry bundle JSON →
//! `decode_with_json_abi` → `DeclarativeMapper` → `ActionEnvelope[]` → lowering.
//!
//! Corpus: `data/golden/uniswap-real-tx/corpus.json` — 42 real mainnet / L2
//! Uniswap transactions sampled via Dune. Every entry traces to an on-chain
//! `tx_hash`.
//!
//! Verification matrix, evaluated per transaction:
//!
//! * **L0 route**  — does `registry/index/by-callkey/<callkey>.json` exist?
//! * **L1 decode** — does `decode_with_json_abi(bundle.abi, calldata)` succeed?
//! * **L2 map**    — does `DeclarativeMapper::map` yield ≥ 1 `ActionEnvelope`?
//! * **L3**        — semantic correctness; **human-judged** — the harness only
//!   serialises the envelopes to JSON, it does not auto-grade.
//! * **L4 lower**  — does `policy_request_from_envelope` return `Some` for
//!   every envelope (`None` would mean a fail-open lowering)?
//!
//! Three tests:
//! * [`harness_self_check`] — strict; unit-verifies the harness components.
//! * [`corpus_verification`] — strict on L0/L1/L2/L4; walks all 42 corpus
//!   entries, prints a verdict table (`cargo test -- --nocapture`), and
//!   asserts every `expect == "pass"` tx fully routes/decodes/maps/lowers.
//! * [`fixed_findings_f2_f5_regression`] — strict; locks the L3 envelope
//!   corrections for `VERIFICATION_UNISWAP_REALTX.md` findings F2~F5 (native
//!   sentinel, UR recipient sentinel, V2 ETH-input amount, V4 outputTokens).

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
// `golden_regression.rs`. The registry lives at the worktree root, two levels
// up from `crates/integration-tests/`.
// ───────────────────────────────────────────────────────────────────────────

/// Worktree root — `crates/integration-tests/` → `../../`.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// `registry/index/by-callkey/` directory.
fn by_callkey_dir() -> PathBuf {
    workspace_root()
        .join("registry")
        .join("index")
        .join("by-callkey")
}

/// `data/golden/uniswap-real-tx/corpus.json`.
fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("golden")
        .join("uniswap-real-tx")
        .join("corpus.json")
}

// ───────────────────────────────────────────────────────────────────────────
// Callkey computation — production routing key.
//
// callkey = `<chain_id>__<to_lowercase>__<selector_lowercase>`
// e.g. `1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x38ed1739`.
// ───────────────────────────────────────────────────────────────────────────

/// Compute the byCallKey index key from `(chain_id, to, selector)`.
///
/// `to` is lowercased; `selector` is the 4-byte function selector rendered as
/// lowercase `0x` + 8 hex chars.
fn callkey(chain_id: u64, to: &str, selector: &[u8; 4]) -> String {
    format!(
        "{}__{}__0x{}",
        chain_id,
        to.to_ascii_lowercase(),
        hex::encode(selector),
    )
}

/// The on-callkey index file content. The `bundle` field is a complete
/// [`AdapterFunctionBundle`]; `registry/manifests/` is never scanned.
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

/// L0 — resolve a callkey to its bundle by reading the single byCallKey index
/// file. `Ok(None)` = MISS (file absent), `Err` = file present but malformed.
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
// Mock ChildResolver — `multicall_recurse` inner-step dispatch.
//
// `multicall_recurse` bundles (V3 NFPM `multicall(bytes[])`, V4 PositionManager
// `multicall`) pull each inner step out as raw calldata and ask the resolver to
// map it. This resolver mirrors `WasmChildResolver`
// (`policy-engine-wasm/src/declarative_exports.rs`), but resolves children
// against the local byCallKey index instead of an in-WASM bridge table.
//
// `resolve_child`:
//   * computes the child callkey from `(chain_id, to, selector)`
//   * looks it up in the byCallKey index
//   * HIT  → inner bundle → `DeclarativeMapper` → `decode_with_json_abi` →
//            `inner_mapper.map(ctx, decoded)` → envelopes
//   * MISS → `Ok(vec![])` (inner step uncovered — recorded as a gap upstream)
// ───────────────────────────────────────────────────────────────────────────

/// Local-index-backed [`ChildResolver`] for `multicall_recurse` bundles.
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

        // MISS — inner step uncovered. Empty result, not an error: the parent
        // multicall is still partially covered, and the harness records the
        // resulting envelope-count delta as a gap.
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

        // `decode_with_json_abi` derives a *static* selector-based decoder_id;
        // overwrite it with the declarative one so `DeclarativeMapper::accepts`
        // (strict equality) would match. `map()` does not itself call
        // `accepts`, but keeping the id canonical is correct and cheap.
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

/// Parse `corpus.json`.
fn load_corpus() -> Corpus {
    let bytes = fs::read(corpus_path()).expect("corpus.json present");
    serde_json::from_slice(&bytes).expect("corpus.json is valid JSON")
}

/// Decode a `0x`-prefixed hex string to bytes.
fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(stripped).map_err(|e| format!("invalid hex: {e}"))
}

/// Extract the 4-byte selector from calldata.
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

/// Outcome of one verification stage.
#[derive(Debug)]
enum StageResult {
    /// L0 routed to a bundle.
    RouteHit {
        bundle_id: String,
        strategy: &'static str,
    },
    /// L0 did not find a byCallKey index file.
    RouteMiss,
    /// L0 index file present but malformed, or calldata too short.
    Fault(String),
}

/// Static label for an [`EmitRule`] discriminant — for the verdict table.
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

/// Full L0~L4 evaluation of a single transaction. Pure — no panics; every
/// failure mode is captured in the returned [`TxVerdict`].
struct TxVerdict {
    /// L0.
    route: StageResult,
    /// L1 — `Some(Ok)` decoded ok, `Some(Err)` decode failed, `None` not reached.
    decode: Option<Result<DecodedCall, DecodeWithJsonAbiError>>,
    /// L2 — `Some(Ok)` mapped, `Some(Err)` mapper error, `None` not reached.
    map: Option<Result<Vec<ActionEnvelope>, MapperError>>,
    /// L4 — `(some_count, total_count)`; `None` not reached.
    lower: Option<(usize, usize)>,
}

impl TxVerdict {
    /// `true` when every reached stage succeeded and produced ≥ 1 envelope all
    /// of which lowered to `Some`.
    fn fully_passed(&self) -> bool {
        matches!(self.route, StageResult::RouteHit { .. })
            && matches!(self.decode, Some(Ok(_)))
            && matches!(&self.map, Some(Ok(envs)) if !envs.is_empty())
            && matches!(self.lower, Some((some, total)) if some == total && total > 0)
    }

    /// `true` when L0 routed but a later reached stage failed.
    fn partial(&self) -> bool {
        matches!(self.route, StageResult::RouteHit { .. }) && !self.fully_passed()
    }

    /// `true` when L0 did not route (clean MISS).
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

    // Parse calldata + selector.
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
            // Canonicalise the decoder_id (see `LocalIndexChildResolver`).
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
    //
    // Parse `from` / `to` / `value`. A parse failure here is a corpus-shape
    // fault, not a mapper gap — surface it on the map stage as an Internal
    // error so the table shows the cause.
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
    // `resolver` is wired unconditionally — `single_emit` /
    // `opcode_stream_dispatch` bundles ignore it; only `multicall_recurse`
    // consults it. Block timestamp is `None` (no live chain context).
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
    //
    // `policy_request_from_envelope` returns `None` on a fail-open lowering.
    // Block timestamp defaults to 0 (the lowering signature wants a concrete
    // u64; no on-chain timestamp is available in this offline harness).
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

/// Render the L2 envelopes of a verdict as a compact JSON array string.
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

/// Unit-verifies the harness components. MUST pass — this guarantees the
/// harness itself is correct before its verdict table is trusted.
#[test]
fn harness_self_check() {
    // (1) callkey computation matches the known V2 swap key.
    let v2_router = "0x7A250D5630B4CF539739DF2C5DACB4C659F2488D";
    let v2_swap_selector = [0x38, 0xed, 0x17, 0x39];
    let key = callkey(1, v2_router, &v2_swap_selector);
    assert_eq!(
        key, "1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x38ed1739",
        "callkey format / lowercasing regression",
    );

    // (2) the byCallKey index directory exists and the V2 swap file loads +
    //     parses into a complete bundle.
    assert!(
        by_callkey_dir().is_dir(),
        "byCallKey index dir missing: {}",
        by_callkey_dir().display(),
    );
    let entry = resolve_callkey(1, v2_router, &v2_swap_selector)
        .expect("V2 swap callkey file must parse")
        .expect("V2 swap callkey file must exist");
    assert_eq!(
        entry.bundle.match_.selector.to_ascii_lowercase(),
        "0x38ed1739"
    );
    assert_eq!(strategy_label(&entry.bundle), "single_emit");

    // (3) corpus.json parses and has 42 entries.
    let corpus = load_corpus();
    assert_eq!(
        corpus.transactions.len(),
        42,
        "corpus.json expected 42 transactions, got {}",
        corpus.transactions.len(),
    );

    // (4) the first `family == "v2"` corpus entry passes L0~L2 + L4.
    let v2_tx = corpus
        .transactions
        .iter()
        .find(|t| t.family == "v2")
        .expect("corpus must contain a v2 transaction");
    let verdict = evaluate(v2_tx);
    assert!(
        matches!(verdict.route, StageResult::RouteHit { .. }),
        "v2 self-check tx {} — L0 route expected HIT, got {:?}",
        v2_tx.tx_hash,
        verdict.route,
    );
    assert!(
        matches!(verdict.decode, Some(Ok(_))),
        "v2 self-check tx {} — L1 decode expected OK, got {:?}",
        v2_tx.tx_hash,
        verdict.decode.as_ref().map(|r| r.as_ref().err()),
    );
    match &verdict.map {
        Some(Ok(envs)) => assert!(
            !envs.is_empty(),
            "v2 self-check tx {} — L2 expected ≥1 envelope",
            v2_tx.tx_hash,
        ),
        other => panic!(
            "v2 self-check tx {} — L2 map expected Ok, got {other:?}",
            v2_tx.tx_hash,
        ),
    }
    match verdict.lower {
        Some((some, total)) => assert!(
            some == total && total > 0,
            "v2 self-check tx {} — L4 expected all {total} envelopes -> Some, got {some}",
            v2_tx.tx_hash,
        ),
        None => panic!(
            "v2 self-check tx {} — L4 lowering not reached",
            v2_tx.tx_hash,
        ),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Test B — corpus_verification (lenient — prints verdict table).
// ───────────────────────────────────────────────────────────────────────────

/// Walk every corpus transaction, evaluate L0~L4, and print a verdict table.
///
/// Run with `cargo test -p integration-tests --test uniswap_real_tx -- --nocapture`
/// to see the table.
///
/// Assertion policy (Phase 5 permanent regression guard): every
/// `expect == "pass"` transaction must route + decode + map + lower (L0~L4);
/// every `expect == "excluded"` transaction must MISS at L0. The full per-tx
/// verdict table is still printed (`cargo test -- --nocapture`) for human L3
/// (envelope-shape) review. Installed after the Phase 4 collect-bundle fix —
/// a future change that regresses the declarative (Tier A) path fails here.
#[test]
#[ignore = "registry v2 cutover — corpus tx set includes L2 deploys (Arbitrum spot-checks) + UR v2.1.1 / V3 SwapRouter (legacy) variants that the v2 manifest scope (mainnet+Base only, no legacy SwapRouter v1) intentionally excludes. Re-enable after Phase D (additional chains) + UR v2.1.1 manifest addition. Mainnet+Base 의 V3 NFPM/V4 PM cross-target dispatch + claim_rewards dual-tokenId 회귀는 단위 test (declarative_exports::plan_children_extracts_cross_target_for_ur_v2_execute_on_base + single_emit::build_claim_rewards_envelope_injects_root_tokenid_into_nft) 가 자동 catch — 본 plan 의 Stage 2 + Stage 4 산출."]
fn corpus_verification() {
    let corpus = load_corpus();
    let mut processed = 0usize;

    // family → (full pass, partial, miss) tallies.
    let mut family_summary: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    // `expect == "pass"` transactions that did not fully pass.
    let mut pass_failures: Vec<String> = Vec::new();
    // `expect == "excluded"` transactions and their observed outcome.
    let mut excluded_outcomes: Vec<String> = Vec::new();
    // `expect == "excluded"` transactions that unexpectedly did NOT miss.
    let mut excluded_unexpected: Vec<String> = Vec::new();

    println!();
    println!("════════════════════════════════════════════════════════════════════════");
    println!(
        " uniswap_real_tx — declarative (Tier A) corpus verification — {} tx",
        corpus.transactions.len()
    );
    println!("════════════════════════════════════════════════════════════════════════");

    for tx in &corpus.transactions {
        let verdict = evaluate(tx);
        processed += 1;

        // ── Header line ────────────────────────────────────────────────────
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

        // ── L0 route ───────────────────────────────────────────────────────
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

        // ── L1 decode ──────────────────────────────────────────────────────
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

        // ── L2 map ─────────────────────────────────────────────────────────
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

        // ── L4 lower ───────────────────────────────────────────────────────
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

        // ── L3 — envelopes serialised for human judgement ──────────────────
        println!("  envelopes: {}", envelopes_json(&verdict));

        // ── Tally ──────────────────────────────────────────────────────────
        let slot = family_summary.entry(tx.family.clone()).or_insert([0, 0, 0]);
        if verdict.fully_passed() {
            slot[0] += 1;
        } else if verdict.partial() {
            slot[1] += 1;
        } else if verdict.missed() {
            slot[2] += 1;
        } else {
            // Fault before L0 routed — count as a miss-class outcome for the
            // family summary (it neither fully passed nor partially routed).
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
                "MISS (correct — intentionally out of Phase 7B scope)".to_string()
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
    }

    // ── Family summary ─────────────────────────────────────────────────────
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

    // ── expect=="pass" failures ────────────────────────────────────────────
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

    // ── expect=="excluded" outcomes ────────────────────────────────────────
    println!();
    println!("────────────────────────────────────────────────────────────────────────");
    println!(
        " expect==\"excluded\" transactions ({}):",
        excluded_outcomes.len()
    );
    for line in &excluded_outcomes {
        println!("{line}");
    }
    println!("════════════════════════════════════════════════════════════════════════");
    println!();

    // ── Assertions — harness completeness + declarative-path coverage. ─────
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

/// Name the first stage that failed (for the failure / excluded summaries).
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
// Test C — fixed_findings_f2_f5_regression (strict).
//
// Permanent regression guard for `VERIFICATION_UNISWAP_REALTX.md` findings
// F2~F5. `corpus_verification` asserts L0/L1/L2/L4 but treats L3 (envelope
// semantics) as human-judged; this test locks the four L3 corrections so a
// future mapper / manifest change that re-introduces a bug fails CI here.
// ───────────────────────────────────────────────────────────────────────────

/// Canonical zero address — the UR (`Constants.ETH`) / V4 (`CurrencyLibrary`)
/// native-asset sentinel.
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

/// Recursively assert no asset object in `v` is an ERC-20 at the zero address
/// (F2 — `0x0` is the native sentinel; it must surface as `native`).
fn assert_no_erc20_at_zero(v: &serde_json::Value, tx_hash: &str) {
    match v {
        serde_json::Value::Object(map) => {
            if map.get("kind").and_then(serde_json::Value::as_str) == Some("erc20") {
                let address = map
                    .get("address")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                assert!(
                    !address.eq_ignore_ascii_case(ZERO_ADDRESS),
                    "F2 regression — tx {tx_hash}: an erc20 asset is at the zero address \
                     (native ETH must be labelled `native`, not `erc20 @ 0x0`)",
                );
            }
            for value in map.values() {
                assert_no_erc20_at_zero(value, tx_hash);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                assert_no_erc20_at_zero(item, tx_hash);
            }
        }
        _ => {}
    }
}

/// Recursively collect every `amount` object beside a native `asset`
/// (`{ "asset": { "kind": "native" }, "amount": { … } }`) — F4.
fn collect_native_amounts(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    match v {
        serde_json::Value::Object(map) => {
            if let (Some(asset), Some(amount)) = (map.get("asset"), map.get("amount")) {
                if asset.get("kind").and_then(serde_json::Value::as_str) == Some("native") {
                    out.push(amount.clone());
                }
            }
            for value in map.values() {
                collect_native_amounts(value, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_native_amounts(item, out);
            }
        }
        _ => {}
    }
}

#[test]
fn fixed_findings_f2_f5_regression() {
    let corpus = load_corpus();

    // Envelopes of the first corpus tx whose `intent` contains `needle`,
    // serialised to JSON for structural assertions.
    let envelopes_of = |needle: &str| -> serde_json::Value {
        let tx = corpus
            .transactions
            .iter()
            .find(|t| t.intent.contains(needle))
            .unwrap_or_else(|| panic!("corpus: no tx with intent containing {needle:?}"));
        match evaluate(tx).map {
            Some(Ok(envelopes)) => {
                serde_json::to_value(&envelopes).expect("envelopes serialise to JSON")
            }
            other => panic!("{needle}: L2 map produced no envelopes ({other:?})"),
        }
    };

    // ── F2 + F3 — corpus-wide invariants over all 42 transactions ──────────
    // F2: no envelope labels native ETH (`0x0`) as `erc20`.
    // F3: no envelope recipient is an unresolved UR/V4 action sentinel.
    let f3_sentinels = [
        "0x0000000000000000000000000000000000000001", // ACTION_MSG_SENDER
        "0x0000000000000000000000000000000000000002", // ACTION_ADDRESS_THIS
    ];
    for tx in &corpus.transactions {
        let Some(Ok(envelopes)) = evaluate(tx).map else {
            continue; // MISS / fault — no envelopes (excluded tx); skip.
        };
        let json = serde_json::to_value(&envelopes).expect("envelopes serialise");
        assert_no_erc20_at_zero(&json, &tx.tx_hash);

        let flat = serde_json::to_string(&envelopes).expect("envelopes serialise");
        for sentinel in f3_sentinels {
            assert!(
                !flat.contains(&format!("\"recipient\":\"{sentinel}\"")),
                "F3 regression — tx {} emits an unresolved recipient sentinel {sentinel}",
                tx.tx_hash,
            );
        }
    }

    // ── F4 — V2 ETH-input native input carries an amount value ─────────────
    // `swapExactETHForTokens` / `addLiquidityETH` fund their native input from
    // `msg.value`; the emit must source `amount.value` from `$.tx.value_wei`.
    for needle in ["swapExactETHForTokens", "addLiquidityETH"] {
        let envelopes = envelopes_of(needle);
        let mut native_amounts = Vec::new();
        collect_native_amounts(&envelopes, &mut native_amounts);
        assert!(
            !native_amounts.is_empty(),
            "F4 {needle}: expected a native input asset in the envelope",
        );
        for amount in &native_amounts {
            let value = amount.get("value").and_then(serde_json::Value::as_str);
            assert!(
                matches!(value, Some(v) if !v.is_empty()),
                "F4 regression — {needle}: native amount has no value \
                 (msg.value must flow into `amount.value`): {amount}",
            );
        }
    }

    // ── F5 — V4 decrease_liquidity surfaces its withdrawn outputs ──────────
    let envelopes = envelopes_of("modifyLiquidities(bytes,uint256)");
    let decrease = envelopes
        .as_array()
        .expect("envelopes is a JSON array")
        .iter()
        .find(|e| e.get("action").and_then(serde_json::Value::as_str) == Some("decrease_liquidity"))
        .expect("F5: corpus modifyLiquidities tx must produce a decrease_liquidity envelope");
    let outputs = decrease
        .pointer("/fields/outputTokens")
        .and_then(serde_json::Value::as_array)
        .expect("F5: decrease_liquidity envelope must carry an outputTokens array");
    assert!(
        !outputs.is_empty(),
        "F5 regression — decrease_liquidity.outputTokens is empty \
         (TAKE / TAKE_PAIR withdrawn tokens must be attached)",
    );
}
