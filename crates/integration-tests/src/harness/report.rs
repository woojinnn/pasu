//! Run report: per-protocol tallies, domain/error histograms, and replayable
//! failure records.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::oracle::{Judged, Verdict};

/// A replayable failure record.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Failure {
    /// Source callkey (or typed-data key) — the routing key.
    pub key: String,
    /// Strategy label.
    pub strategy: String,
    /// Seed that produced this case (replayable with `replay --seed`).
    pub seed: u64,
    /// Oracle layer (or `Panic`).
    pub layer: String,
    /// Detail message.
    pub detail: String,
    /// Exact calldata / message that triggered it.
    pub input: String,
}

/// Per-protocol tally (protocol = first path segment of the bundle id).
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ProtoStat {
    /// Cases run.
    pub total: usize,
    /// `ok:true`, valid.
    pub pass: usize,
    /// Tolerated soft errors.
    pub soft: usize,
    /// Hard oracle failures.
    pub fail: usize,
    /// Panics.
    pub panicked: usize,
}

/// Aggregated run report.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Report {
    /// Cases run (excludes harness skips).
    pub total: usize,
    /// Passes.
    pub pass: usize,
    /// Soft errors.
    pub soft: usize,
    /// Hard failures.
    pub fail: usize,
    /// Panics.
    pub panicked: usize,
    /// Cases skipped because the harness could not build args (e.g. an ABI type
    /// it does not model) — not a decode finding.
    pub skipped: usize,
    /// `body.domain` → count (incl. `unknown`).
    pub domain_hist: BTreeMap<String, usize>,
    /// `error.kind` → count.
    pub error_hist: BTreeMap<String, usize>,
    /// protocol → tally.
    pub per_protocol: BTreeMap<String, ProtoStat>,
    /// Replayable failures.
    pub failures: Vec<Failure>,
}

fn protocol_of(bundle_id: &str) -> String {
    bundle_id.split('/').next().unwrap_or("?").to_owned()
}

impl Report {
    /// Record one judged case.
    pub fn record(
        &mut self,
        key: &str,
        bundle_id: &str,
        strategy: &str,
        seed: u64,
        input: &str,
        judged: &Judged,
    ) {
        self.total += 1;
        let ps = self.per_protocol.entry(protocol_of(bundle_id)).or_default();
        ps.total += 1;
        for d in &judged.domains {
            *self.domain_hist.entry(d.clone()).or_default() += 1;
        }
        if let Some(k) = &judged.error_kind {
            *self.error_hist.entry(k.clone()).or_default() += 1;
        }
        match &judged.verdict {
            Verdict::Pass => {
                self.pass += 1;
                ps.pass += 1;
            }
            Verdict::SoftError { .. } => {
                self.soft += 1;
                ps.soft += 1;
            }
            Verdict::Fail { layer, detail } => {
                self.fail += 1;
                ps.fail += 1;
                self.failures.push(Failure {
                    key: key.to_owned(),
                    strategy: strategy.to_owned(),
                    seed,
                    layer: format!("{layer:?}"),
                    detail: detail.clone(),
                    input: input.to_owned(),
                });
            }
        }
    }

    /// Record a panic recovered by `catch_unwind`.
    pub fn record_panic(
        &mut self,
        key: &str,
        bundle_id: &str,
        strategy: &str,
        seed: u64,
        input: &str,
    ) {
        self.total += 1;
        self.panicked += 1;
        let ps = self.per_protocol.entry(protocol_of(bundle_id)).or_default();
        ps.total += 1;
        ps.panicked += 1;
        self.failures.push(Failure {
            key: key.to_owned(),
            strategy: strategy.to_owned(),
            seed,
            layer: "Panic".to_owned(),
            detail: "route panicked".to_owned(),
            input: input.to_owned(),
        });
    }

    /// Record a harness skip (could not build args).
    pub fn record_skip(&mut self) {
        self.skipped += 1;
    }

    /// Hard findings = oracle failures + panics. The CI gate asserts this is 0.
    #[must_use]
    pub const fn hard_failures(&self) -> usize {
        self.fail + self.panicked
    }

    /// `unknown`-domain share, as a metric (surfaces the HyperLiquid limitation).
    #[must_use]
    pub fn unknown_pct(&self) -> f64 {
        let total: usize = self.domain_hist.values().sum();
        if total == 0 {
            0.0
        } else {
            100.0 * (*self.domain_hist.get("unknown").unwrap_or(&0) as f64) / (total as f64)
        }
    }

    /// Human-readable multi-line summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "total={} pass={} soft={} fail={} panicked={} skipped={}",
            self.total, self.pass, self.soft, self.fail, self.panicked, self.skipped
        );
        let _ = writeln!(s, "\nper-protocol:");
        for (proto, st) in &self.per_protocol {
            let _ = writeln!(
                s,
                "  {proto:<14} total={:<5} pass={:<5} soft={:<4} fail={:<3} panic={}",
                st.total, st.pass, st.soft, st.fail, st.panicked
            );
        }
        let _ = writeln!(
            s,
            "\ndomain histogram (unknown={:.1}%):",
            self.unknown_pct()
        );
        for (d, c) in &self.domain_hist {
            let _ = writeln!(s, "  {d:<12} {c}");
        }
        if !self.error_hist.is_empty() {
            let _ = writeln!(s, "\nerror histogram:");
            for (k, c) in &self.error_hist {
                let _ = writeln!(s, "  {k:<40} {c}");
            }
        }
        if !self.failures.is_empty() {
            // Cap how many failures we print (override with V3_HARNESS_MAX_FAILURES).
            let cap = std::env::var("V3_HARNESS_MAX_FAILURES")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(20);
            let shown = self.failures.len().min(cap);
            let _ = writeln!(s, "\nfailures (showing {shown}/{}):", self.failures.len());
            for f in self.failures.iter().take(shown) {
                let _ = writeln!(
                    s,
                    "  [{}] {} seed={} {}\n      input={}",
                    f.layer, f.key, f.seed, f.detail, f.input
                );
            }
        }
        s
    }
}
