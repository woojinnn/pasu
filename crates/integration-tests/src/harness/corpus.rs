//! Real-transaction corpus replay.
//!
//! Loads `data/golden/v3-decode/<protocol>/corpus.json` (real on-chain txs +
//! captured EIP-712 signatures), routes each through the v3 entrypoint, and
//! checks the result against the per-entry `expect`. This is the positive
//! verification path for entries the synthetic fuzzer defers (typed-data field
//! coercion, UR Permit2-embedding commands, V4 nested) — real payloads have the
//! exact shapes the decoder expects.
//!
//! Format (`{transactions: [...]}`, extending the existing real-tx corpus):
//! ```jsonc
//! { "expect": "pass" | "excluded" | "error",
//!   "expect_domain": "amm",            // optional: top-level body.domain
//!   "expect_action": "swap",           // optional: flat action tag
//!   "expect_body": [                   // optional: protocol-agnostic field assertions
//!     {"path":"$.data.actions[0].body.domain","op":"equals","value":"amm"} ],
//!   "expect_error": "decode_failed",   // optional: only when expect=="error"
//!   "chain_id": 1, "tx_hash": "0x..",
//!   "rpc": { "params": [{ "from","to","value","data" }] },
//!   // OR, for an EIP-712 signature:
//!   "typed_data": { "verifying_contract","primary_type","witness_type"?,
//!                   "domain_name"?, "message": {...} } }
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::harness::oracle::{judge, Judged, Verdict};
use crate::harness::semantic::{self, BodyAssertion};
use crate::harness::{adapters, route};

#[derive(Debug, Deserialize)]
struct CorpusFile {
    transactions: Vec<CorpusTx>,
}

#[derive(Debug, Deserialize)]
struct CorpusTx {
    #[serde(default)]
    intent: String,
    expect: String,
    #[serde(default)]
    expect_domain: Option<String>,
    #[serde(default)]
    expect_action: Option<String>,
    #[serde(default)]
    expect_body: Vec<BodyAssertion>,
    #[serde(default)]
    expect_error: Option<String>,
    #[serde(default)]
    tx_hash: String,
    chain_id: u64,
    #[serde(default)]
    rpc: Option<Rpc>,
    #[serde(default)]
    typed_data: Option<TypedDataTx>,
}

#[derive(Debug, Deserialize)]
struct Rpc {
    params: Vec<Param>,
}

#[derive(Debug, Deserialize)]
struct Param {
    to: String,
    #[serde(default = "zero")]
    value: String,
    data: String,
}

fn zero() -> String {
    "0".to_owned()
}

#[derive(Debug, Deserialize)]
struct TypedDataTx {
    verifying_contract: String,
    primary_type: String,
    #[serde(default)]
    witness_type: Option<String>,
    #[serde(default)]
    domain_name: Option<String>,
    message: Value,
}

/// One corpus entry's verdict against its expectation.
#[derive(Debug, Clone)]
pub struct CorpusOutcome {
    /// `tx_hash` or `intent` label.
    pub label: String,
    /// Source corpus file.
    pub source: String,
    /// Expectation string.
    pub expect: String,
    /// What the harness actually produced.
    pub got: String,
    /// Whether the expectation was met.
    pub matched: bool,
}

/// Run every `corpus.json` under `root`, returning per-entry outcomes.
///
/// Installs the local adapter surface first (same thread — R1), then routes each
/// transaction / signature.
pub fn run_corpus(root: &Path) -> Result<Vec<CorpusOutcome>> {
    // Install adapters on this thread so routing resolves.
    let _surface = adapters::load_and_install()?;
    let mut outcomes = Vec::new();
    for file in find_corpus_files(root)? {
        let raw = fs::read_to_string(&file).with_context(|| format!("read {}", file.display()))?;
        let parsed: CorpusFile =
            serde_json::from_str(&raw).with_context(|| format!("parse {}", file.display()))?;
        let source = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .into_owned();
        for tx in parsed.transactions {
            outcomes.push(run_tx(&tx, &source));
        }
    }
    Ok(outcomes)
}

fn find_corpus_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }
    // One level of <protocol>/ subdirs (+ root itself).
    let mut dirs = vec![root.to_path_buf()];
    for entry in fs::read_dir(root)?.filter_map(std::result::Result::ok) {
        if entry.path().is_dir() {
            dirs.push(entry.path());
        }
    }
    for dir in dirs {
        let f = dir.join("corpus.json");
        if f.is_file() {
            out.push(f);
        }
    }
    out.sort();
    Ok(out)
}

fn run_tx(tx: &CorpusTx, source: &str) -> CorpusOutcome {
    let label = if tx.tx_hash.is_empty() {
        tx.intent.clone()
    } else {
        tx.tx_hash.clone()
    };

    let env = if let Some(td) = &tx.typed_data {
        route::route_typed_data(
            tx.chain_id,
            &td.verifying_contract,
            &td.primary_type,
            td.witness_type.as_deref(),
            td.domain_name.as_deref(),
            &td.message,
        )
    } else if let Some(rpc) = &tx.rpc {
        let Some(p) = rpc.params.first() else {
            return CorpusOutcome {
                label,
                source: source.to_owned(),
                expect: tx.expect.clone(),
                got: "no rpc params".to_owned(),
                matched: false,
            };
        };
        let selector = if p.data.len() >= 10 {
            p.data[..10].to_lowercase()
        } else {
            "0x00000000".to_owned()
        };
        route::route_calldata(tx.chain_id, &p.to, &selector, &p.data, &p.value)
    } else {
        return CorpusOutcome {
            label,
            source: source.to_owned(),
            expect: tx.expect.clone(),
            got: "entry has neither rpc nor typed_data".to_owned(),
            matched: false,
        };
    };

    let judged = judge(&env);
    let (matched, got) = check_expect(tx, &judged, &env);
    CorpusOutcome {
        label,
        source: source.to_owned(),
        expect: tx.expect.clone(),
        got,
        matched,
    }
}

fn top_domain(judged: &Judged) -> Option<&str> {
    judged.domains.first().map(String::as_str)
}

fn check_expect(tx: &CorpusTx, judged: &Judged, envelope: &Value) -> (bool, String) {
    match tx.expect.as_str() {
        "pass" => match &judged.verdict {
            Verdict::Pass => {
                if let Some(want) = &tx.expect_domain {
                    if top_domain(judged) != Some(want.as_str()) {
                        return (
                            false,
                            format!("pass but domain={:?} (want {want})", top_domain(judged)),
                        );
                    }
                }
                let _ = &tx.expect_action; // reserved (flat action tag) — pinned via domain for now
                if !tx.expect_body.is_empty() {
                    if let Err(e) = semantic::check_expect_body(envelope, &tx.expect_body) {
                        return (false, format!("pass but expect_body failed: {e}"));
                    }
                }
                (true, "pass".to_owned())
            }
            Verdict::SoftError { kind } => (false, format!("soft({kind}) — expected pass")),
            Verdict::Fail { layer, detail } => (false, format!("FAIL[{layer:?}] {detail}")),
        },
        "excluded" => match &judged.verdict {
            Verdict::Pass if top_domain(judged) == Some("unknown") => (true, "excluded".to_owned()),
            other => (false, format!("expected excluded(unknown), got {other:?}")),
        },
        "error" => match &judged.verdict {
            Verdict::Fail { .. } | Verdict::SoftError { .. } => {
                if let Some(want) = &tx.expect_error {
                    if judged.error_kind.as_deref() != Some(want.as_str()) {
                        return (
                            false,
                            format!("error kind={:?} (want {want})", judged.error_kind),
                        );
                    }
                }
                (true, "error".to_owned())
            }
            Verdict::Pass => (false, "expected error, got pass".to_owned()),
        },
        other => (false, format!("unknown expect `{other}`")),
    }
}
