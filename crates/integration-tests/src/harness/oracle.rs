//! Layered oracle: what "the `ActionBody[]` is produced correctly" means with
//! no RPC server (so `live_inputs.value` is intentionally empty).
//!
//! Evaluated in order; the first failing layer wins:
//! * **L1 Envelope** — output parses and has an `ok` field.
//! * **L2 TypedRoundTrip** *(strongest)* — `data.actions` re-deserializes into
//!   the real `Vec<policy_transition::action::Action>`. A failure here is a
//!   serde-shape regression even when the envelope said `ok:true`.
//! * **L3 Domain** — every emitted `body.domain` is one of the valid domains.
//!   `unknown` is **valid** (it is the correct output for HyperLiquid off-chain
//!   ops / native transfers) — counted as a metric, never failed.
//! * **L4 ErrorClass** — for `ok:false`, soft errors (`no_declarative_v3_mapper`
//!   etc.) are tolerated; hard builder errors are findings.
//!
//! Corpus-mode `expect_*` comparison lives in `corpus.rs`, layered on top of
//! [`judge`].

use policy_transition::action::Action;
use serde_json::Value;

/// The valid `ActionBody` domains (serde `domain` tags).
pub const VALID_DOMAINS: [&str; 14] = [
    "token",
    "amm",
    "lending",
    "airdrop",
    "launchpad",
    "liquid_staking",
    "perp",
    "permission",
    "yield",
    "restaking",
    "staking",
    "hyperliquid_core",
    "multicall",
    "unknown",
];

/// Engine error kinds the harness tolerates (not findings): an unrouted/unknown
/// key or a strategy that typed-data routing legitimately rejects.
pub const SOFT_ERROR_KINDS: [&str; 3] = [
    "no_declarative_v3_mapper",
    "unsupported_strategy_for_typed_data",
    "no_typed_data_mapper",
];

/// Which layer produced a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OracleLayer {
    /// Envelope missing/garbled.
    Envelope,
    /// `data.actions` did not re-type into `Vec<Action>`.
    TypedRoundTrip,
    /// An emitted domain tag is not one of the valid domains.
    Domain,
    /// `ok:false` with a hard engine error.
    ErrorClass,
}

/// Per-transaction verdict.
#[derive(Clone, Debug)]
pub enum Verdict {
    /// `ok:true`, round-trips, all domains valid.
    Pass,
    /// `ok:false` with a tolerated soft error.
    SoftError {
        /// The tolerated `error.kind`.
        kind: String,
    },
    /// A hard finding.
    Fail {
        /// Layer that flagged it.
        layer: OracleLayer,
        /// Human detail.
        detail: String,
    },
}

/// The verdict plus side metrics the reporter aggregates.
#[derive(Clone, Debug)]
pub struct Judged {
    /// Pass / soft-error / fail.
    pub verdict: Verdict,
    /// Top-level `body.domain` of each emitted action (histogram input).
    pub domains: Vec<String>,
    /// `error.kind` when `ok:false`.
    pub error_kind: Option<String>,
}

/// R4: a `build_*_failed` whose message is an array index out of bounds is a
/// malformed-input artifact (the body indexes `$args.arr[i]` but the synthetic
/// array was too short / empty — a would-revert tx), not a decode bug. Tolerate
/// it (still counted in the error histogram for visibility).
fn is_shape_artifact(kind: &str, msg: &str) -> bool {
    matches!(
        kind,
        "build_action_body_failed" | "build_array_emit_failed" | "build_multicall_failed"
    ) && (
        // `$args.arr[i]` over a too-short synthetic array (would-revert tx).
        msg.contains("out of bounds")
        // value-map discriminant ($match) not in $cases and no $default — a
        // random out-of-enum discriminant (e.g. interestRateMode=999). Real
        // discriminants are exercised by the corpus; tolerate here.
        || msg.contains("value-map: no case")
        // `$fn` executors over a synthetic input: random calldata yields an
        // all-zero Curve route or an out-of-enum swap_type. Real routes are
        // exercised by the corpus/golden (which assert the resolved token); a
        // STRUCTURAL $fn bug (unknown fn / bad arg wiring) errors on EVERY input
        // incl. the golden, so it is NOT masked by these data-only patterns.
        || msg.contains("empty route (no non-zero pool slot)")
        || msg.contains("unknown swap_type")
        || msg.contains("missing swap_params")
    )
}

fn collect_top_domains(actions: &Value, out: &mut Vec<String>) {
    if let Some(arr) = actions.as_array() {
        for a in arr {
            if let Some(d) = a
                .get("body")
                .and_then(|b| b.get("domain"))
                .and_then(Value::as_str)
            {
                out.push(d.to_owned());
            }
        }
    }
}

/// Judge a parsed route envelope.
#[must_use]
pub fn judge(envelope: &Value) -> Judged {
    let mut domains = Vec::new();
    match envelope.get("ok").and_then(Value::as_bool) {
        Some(true) => {
            let actions = envelope
                .get("data")
                .and_then(|d| d.get("actions"))
                .cloned()
                .unwrap_or(Value::Null);
            collect_top_domains(&actions, &mut domains);

            // L2 — strongest: re-type into the real Action enum.
            if let Err(e) = serde_json::from_value::<Vec<Action>>(actions) {
                return Judged {
                    verdict: Verdict::Fail {
                        layer: OracleLayer::TypedRoundTrip,
                        detail: e.to_string(),
                    },
                    domains,
                    error_kind: None,
                };
            }

            // L3 — domain tag validity (round-trip already guarantees this, but
            // an explicit check guards against future enum drift / aliasing).
            for d in &domains {
                if !VALID_DOMAINS.contains(&d.as_str()) {
                    return Judged {
                        verdict: Verdict::Fail {
                            layer: OracleLayer::Domain,
                            detail: format!("invalid domain `{d}`"),
                        },
                        domains: domains.clone(),
                        error_kind: None,
                    };
                }
            }

            Judged {
                verdict: Verdict::Pass,
                domains,
                error_kind: None,
            }
        }
        Some(false) => {
            let err = envelope.get("error");
            let kind = err
                .and_then(|e| e.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            let msg = err
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if SOFT_ERROR_KINDS.contains(&kind.as_str()) || is_shape_artifact(&kind, &msg) {
                Judged {
                    verdict: Verdict::SoftError { kind: kind.clone() },
                    domains,
                    error_kind: Some(kind),
                }
            } else {
                Judged {
                    verdict: Verdict::Fail {
                        layer: OracleLayer::ErrorClass,
                        detail: format!("{kind}: {msg}"),
                    },
                    domains,
                    error_kind: Some(kind),
                }
            }
        }
        None => Judged {
            verdict: Verdict::Fail {
                layer: OracleLayer::Envelope,
                detail: format!("missing `ok` field in envelope: {envelope}"),
            },
            domains,
            error_kind: None,
        },
    }
}
