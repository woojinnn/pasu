//! Service DTO contract for the simulation backend.
//!
//! These are the JSON request/response shapes the browser extension and the
//! backend agree on. They match the action-model `scopeball.evaluate_v3`
//! contract (`wallet_id` / `actions` / `eval_context`
//! → `policyRequest{actions,state_before,deltas,state_after}` / `diagnostics`),
//! adding one field the simulation backend needs:
//!   - request `call_specs` — the enrichment calls the extension's
//!     manifest-planning decided; the backend EXECUTES them.
//!   - response `policyRequest.results` — the executed results keyed by
//!     `call_id`, which the extension feeds to the WASM `evaluate_action_v2_json`
//!     materialize step.
//!
//! The backend never evaluates Cedar — it returns state/statediff/results and
//! the extension produces the verdict.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use simulation_reducer::action::Action;
use simulation_state::primitives::ChainId;
use simulation_state::{EvalContext, StateDelta, WalletId, WalletState};

/// Request: browser extension → simulation backend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvaluateRequest {
    /// Wallet identity (address + tracked chains) the simulation runs against.
    pub wallet_id: WalletId,
    /// Caller-built Actions — decoded calldata/signature → typed `Action`
    /// (meta + `ActionBody`). The backend simulates each in order.
    pub actions: Vec<Action>,
    /// Per-evaluation context (chain, time, request kind, simulation mode).
    pub eval_context: EvalContext,
    /// Enrichment calls the extension's manifest-planning produced; the backend
    /// EXECUTES each and returns its raw result keyed by `call_id`. Empty when
    /// no policy requires enrichment.
    #[serde(default)]
    pub call_specs: Vec<CallSpec>,
}

/// A single enrichment call the backend must execute — the policy-RPC "plan"
/// the extension derived from a policy manifest. Mirrors the WASM
/// `PlannedCallV2` shape so the extension forwards it unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallSpec {
    /// Manifest that produced this call (for diagnostics / dedup).
    pub manifest_id: String,
    /// Unique id; the executed result is returned under this key in
    /// [`PolicyRequest::results`].
    pub call_id: String,
    /// Enrichment method to invoke (e.g. an oracle USD-value lookup).
    pub method: String,
    /// Resolved method parameters (selectors already resolved by the extension).
    pub params: Value,
    /// Output→context projections — **opaque** to the backend. The extension
    /// materializes these into the Cedar `context.custom` after evaluation; the
    /// backend only needs `method` + `params` to execute. Carried through so the
    /// extension can forward a single spec to both sides.
    #[serde(default)]
    pub outputs: Vec<Value>,
    /// When true, a failed call is skipped (surfaced as a diagnostic) rather
    /// than failing the whole request.
    #[serde(default)]
    pub optional: bool,
}

/// Response: simulation backend → browser extension.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvaluateResponse {
    /// The post-processed policy request the extension feeds to its WASM Cedar
    /// layer (typed actions + simulated state/diff + executed enrichment
    /// results). Renamed to `policyRequest` to match the v3 wire contract.
    #[serde(rename = "policyRequest")]
    pub policy_request: PolicyRequest,
    /// Non-fatal diagnostics (failed optional calls, stale data, …).
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// The simulated outcome the extension's Cedar layer consumes. Field names
/// match the v3 `policyRequest` (with `results` added).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PolicyRequest {
    /// Typed action(s), echoed from the request `actions`.
    pub actions: Vec<Action>,
    /// Wallet state before applying the action(s).
    pub state_before: WalletState,
    /// One state delta per action — the simulated/predicted change
    /// (`reducer::apply`), not an authoritative ledger update.
    pub deltas: Vec<StateDelta>,
    /// Predicted wallet state after applying the action(s) in memory. The server
    /// does not persist this as canonical state.
    pub state_after: WalletState,
    /// Executed enrichment results keyed by [`CallSpec::call_id`] — feeds the
    /// extension's WASM `evaluate_action_v2_json` materialize step.
    #[serde(default)]
    pub results: BTreeMap<String, Value>,
}

/// A non-fatal diagnostic returned alongside the result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Severity, e.g. `"warn"` | `"info"`.
    pub level: String,
    /// Human-readable message.
    pub message: String,
    /// The `call_id` this diagnostic relates to, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}

/// Request: browser extension → simulation backend after an action leaves the
/// policy decision point.
///
/// This is intentionally separate from [`EvaluateRequest`]. `evaluate` returns
/// a prediction for Cedar, while execution reports describe what happened next:
/// a wallet signature, an on-chain submission, or a venue response. Some venues
/// such as Hyperliquid can be submitted through an already-approved agent key,
/// so no browser wallet signature is required before a venue report arrives.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReportRequest {
    /// Wallet identity the reported action belongs to, when the reporter can
    /// prove it.
    ///
    /// This is optional because Hyperliquid `/exchange` requests may be signed
    /// by an already-approved agent key. The extension can observe the venue
    /// response, but the request body alone does not always reveal the master
    /// account whose venue state should later be reconciled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallet_id: Option<WalletId>,
    /// Optional extension-generated evaluation id. The Rust server does not
    /// require this today, but carrying it lets a DB-backed implementation link
    /// reports to the exact policy evaluation that authorized the action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_id: Option<String>,
    /// Zero-based index into [`EvaluateRequest::actions`] when the report is
    /// tied to one action in a batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_index: Option<usize>,
    /// Lifecycle event reported by the extension.
    pub outcome: ExecutionReportOutcome,
    /// Opaque structured details from the extension, wallet, chain, or venue.
    /// Keep canonical fields in [`ExecutionReportOutcome`]; use this only for
    /// provider-specific payloads that should still be persisted for audit.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

/// The post-policy execution lifecycle event observed by the extension.
///
/// Wallet outcomes cover MetaMask-style flows. Venue outcomes cover systems
/// where the extension observes a direct venue API response, including
/// Hyperliquid `/exchange` orders that can be authorized by an agent key rather
/// than a fresh browser-wallet signature.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionReportOutcome {
    /// The browser wallet refused, cancelled, or failed before producing a
    /// signature/transaction hash.
    WalletRejected {
        /// Optional wallet/provider reason shown to the user or returned by the
        /// wallet API.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The browser wallet produced an off-chain signature.
    WalletSigned {
        /// Signature bytes as a `0x`-prefixed string.
        signature: String,
    },
    /// The wallet/provider submitted a transaction to an on-chain network.
    OnchainSubmitted {
        /// Chain where the transaction was submitted.
        chain: ChainId,
        /// Transaction hash as a `0x`-prefixed string.
        tx_hash: String,
    },
    /// A transaction was observed as confirmed.
    OnchainConfirmed {
        /// Chain where the transaction confirmed.
        chain: ChainId,
        /// Transaction hash as a `0x`-prefixed string.
        tx_hash: String,
        /// Confirming block number, when the reporter knows it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block_number: Option<u64>,
    },
    /// The extension submitted a request to an off-chain venue.
    VenueSubmitted {
        /// Venue name, e.g. `"hyperliquid"`.
        venue: String,
        /// Client order id / cloid / correlation id, when present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_order_id: Option<String>,
    },
    /// The off-chain venue accepted the request.
    VenueAccepted {
        /// Venue name, e.g. `"hyperliquid"`.
        venue: String,
        /// Venue-assigned order id, when present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        venue_order_id: Option<String>,
        /// Client order id / cloid / correlation id, when present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_order_id: Option<String>,
    },
    /// The off-chain venue rejected the request.
    VenueRejected {
        /// Venue name, e.g. `"hyperliquid"`.
        venue: String,
        /// Venue/provider rejection reason.
        reason: String,
    },
    /// Catch-all failure after policy approval when the extension cannot map
    /// the error to a more precise wallet/chain/venue stage.
    Failed {
        /// Failure reason captured by the extension.
        reason: String,
    },
}

/// Response returned after the server records an [`ExecutionReportRequest`].
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReportResponse {
    /// Whether the report was accepted for recording.
    pub accepted: bool,
    /// Whether this report directly mutated canonical wallet state.
    ///
    /// This stays `false` for the current server. Reports are lifecycle facts;
    /// canonical state still comes from authoritative chain receipts or venue
    /// sync snapshots.
    pub canonical_state_updated: bool,
    /// Non-fatal diagnostics about how the report should be interpreted.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_state::primitives::{Address, ChainId, Time};
    use simulation_state::RequestKind;

    fn sample_wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [ChainId::ethereum_mainnet()],
        )
    }

    fn sample_request() -> EvaluateRequest {
        EvaluateRequest {
            wallet_id: sample_wallet_id(),
            actions: Vec::new(),
            eval_context: EvalContext::new(
                ChainId::ethereum_mainnet(),
                Time::from_unix(1_700_000_000),
                RequestKind::Transaction,
            ),
            call_specs: vec![CallSpec {
                manifest_id: "swap-usd-guard".into(),
                call_id: "swap-usd-guard::oracle".into(),
                method: "oracle.usd_value".into(),
                params: serde_json::json!({ "token": "USDC", "amount": "0x3b9aca00" }),
                outputs: vec![
                    serde_json::json!({ "from": "$.result.usd", "into": "totalInputUsd" }),
                ],
                optional: false,
            }],
        }
    }

    fn sample_response() -> EvaluateResponse {
        let mut results = BTreeMap::new();
        results.insert(
            "swap-usd-guard::oracle".to_owned(),
            serde_json::json!({ "usd": "3500.1200" }),
        );
        EvaluateResponse {
            policy_request: PolicyRequest {
                actions: Vec::new(),
                state_before: WalletState::new(sample_wallet_id()),
                deltas: Vec::new(),
                state_after: WalletState::new(sample_wallet_id()),
                results,
            },
            diagnostics: vec![Diagnostic {
                level: "info".into(),
                message: "0 optional calls skipped".into(),
                call_id: None,
            }],
        }
    }

    /// The wire field names match (+ extend) the `scopeball.evaluate_v3`
    /// contract: request `wallet_id`/`actions`/`eval_context`/`call_specs`,
    /// response `policyRequest{actions,state_before,deltas,state_after,results}`.
    #[test]
    fn wire_field_names_match_v3_contract() {
        let rv = serde_json::to_value(sample_request()).unwrap();
        for k in ["wallet_id", "actions", "eval_context", "call_specs"] {
            assert!(rv.get(k).is_some(), "request missing `{k}`");
        }

        let pv = serde_json::to_value(sample_response()).unwrap();
        assert!(
            pv.get("policyRequest").is_some(),
            "response missing `policyRequest`"
        );
        assert!(
            pv.get("diagnostics").is_some(),
            "response missing `diagnostics`"
        );
        let pr = &pv["policyRequest"];
        for k in [
            "actions",
            "state_before",
            "deltas",
            "state_after",
            "results",
        ] {
            assert!(pr.get(k).is_some(), "policyRequest missing `{k}`");
        }
    }

    /// Request + response round-trip through JSON unchanged.
    #[test]
    fn dto_json_round_trip() {
        let req = sample_request();
        let back: EvaluateRequest =
            serde_json::from_value(serde_json::to_value(&req).unwrap()).unwrap();
        assert_eq!(req, back);

        let resp = sample_response();
        let back: EvaluateResponse =
            serde_json::from_value(serde_json::to_value(&resp).unwrap()).unwrap();
        assert_eq!(resp, back);
    }

    /// Optional request fields (`call_specs`) and response `results` default to
    /// empty when omitted — a minimal v3-style payload still deserializes.
    #[test]
    fn optional_fields_default_when_omitted() {
        let req = sample_request();
        let mut rv = serde_json::to_value(&req).unwrap();
        rv.as_object_mut().unwrap().remove("call_specs");
        let parsed: EvaluateRequest = serde_json::from_value(rv).unwrap();
        assert!(parsed.call_specs.is_empty());
    }

    /// Execution reports must support ordinary wallet callbacks and
    /// Hyperliquid-style venue callbacks where no browser wallet signature was
    /// involved.
    #[test]
    fn execution_report_outcomes_round_trip() {
        let wallet_signed = ExecutionReportRequest {
            wallet_id: Some(sample_wallet_id()),
            evaluation_id: Some("eval-1".to_owned()),
            action_index: Some(0),
            outcome: ExecutionReportOutcome::WalletSigned {
                signature: "0xdeadbeef".to_owned(),
            },
            metadata: BTreeMap::new(),
        };

        let venue_accepted = ExecutionReportRequest {
            wallet_id: Some(sample_wallet_id()),
            evaluation_id: Some("eval-2".to_owned()),
            action_index: Some(0),
            outcome: ExecutionReportOutcome::VenueAccepted {
                venue: "hyperliquid".to_owned(),
                venue_order_id: Some("123456".to_owned()),
                client_order_id: None,
            },
            metadata: BTreeMap::new(),
        };

        for report in [wallet_signed, venue_accepted] {
            let value = serde_json::to_value(&report).unwrap();
            assert!(
                value["outcome"]["kind"].is_string(),
                "execution report outcome must be tagged"
            );
            let back: ExecutionReportRequest = serde_json::from_value(value).unwrap();
            assert_eq!(back, report);
        }
    }
}
