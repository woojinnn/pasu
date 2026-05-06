//! Pipeline orchestrator wiring stages 1–4 together for v0.1.
//!
//! ```text
//!   TransactionRequest
//!     → Stage 1 (Adapter Resolver)
//!     → Stage 2+3+4-prep (Adapter::into_requests)
//!     → Stage 4 (Cedar evaluator)
//!     → Verdict
//! ```
//!
//! Each stage's output is the next stage's input. v0.1's failure model is
//! fail-closed: most pipeline-level errors propagate as `Err(...)` rather
//! than being silently downgraded to `Verdict::Allow`.

use crate::core::{Action, TransactionRequest};
use crate::host::HostCapabilities;
use crate::lowering::{
    compute_swap_window_deltas, enrich_tx_request_with_window_stats, request_for_tx, request_from_action,
};
use crate::policy::{PolicyEngine, PolicyError, PolicyRequest, RequestKind, Verdict};
use crate::stat_windows::StatKey;
use crate::stat_windows::ReservationId;
use crate::registry::{AdapterRegistry, ResolverOutcome};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("adapter ambiguity: {0:?}")]
    Ambiguous(Vec<crate::AdapterId>),
    #[error("adapter build failed: {0}")]
    AdapterBuild(String),
    #[error("policy evaluation failed: {0}")]
    Policy(#[from] PolicyError),
}

/// Pipeline is generic over the registry type — `R: ?Sized` lets callers
/// pass either a concrete `&MockAdapterRegistry` (monomorphized, fast) or a
/// `&dyn AdapterRegistry` trait object (dynamic dispatch, swappable at
/// runtime). Host capabilities are passed as a small borrowed bundle to avoid
/// `Pipeline::new` signature churn as capabilities expand.
pub struct Pipeline<'a, R: AdapterRegistry + ?Sized> {
    pub registry: &'a R,
    pub host: HostCapabilities<'a>,
    pub policies: &'a PolicyEngine,
}

pub struct EvaluationOutcome {
    pub verdict: Verdict,
    pub reservation: Option<ReservationId>,
}

impl<'a, R: AdapterRegistry + ?Sized> Pipeline<'a, R> {
    pub fn new(
        registry: &'a R,
        host: HostCapabilities<'a>,
        policies: &'a PolicyEngine,
    ) -> Self {
        Pipeline {
            registry,
            host,
            policies,
        }
    }

    fn build_requests(
        &self,
        tx: &TransactionRequest,
    ) -> Result<
        (Vec<Action>, Vec<PolicyRequest>, PolicyRequest, Vec<(PolicyRequest, RequestKind)>),
        PipelineError,
    > {
        let (outcome, adapter) = self.registry.resolve_with_adapter(tx);
        
        let (leaves, leaf_requests) = match (outcome, adapter) {
            (ResolverOutcome::Ambiguous(ids), _) => {
                return Err(PipelineError::Ambiguous(ids));
            }
            (ResolverOutcome::NoMatch, _) => {
                // No adapter matched — emit `Action::Other` and let user
                // policies decide whether to allow unrecognized calls.
                let action = Action::Other {
                    actor: tx.from.clone(),
                    target: tx.to.clone(),
                    selector: tx.selector_hex().unwrap_or_else(|| "0x".into()),
                    value_wei: tx.value_wei.clone(),
                    raw_calldata: format!("0x{}", hex::encode(&tx.data)),
                };
                let leaves = vec![action];
                let leaf_requests = vec![request_from_action(&leaves[0])];
                (leaves, leaf_requests)
            }
            (ResolverOutcome::Resolved(_), Some(adapter)) => {
                let leaves = adapter
                    .build_actions(tx)
                    .map_err(|e| PipelineError::AdapterBuild(e.to_string()))?;
                let leaf_requests = adapter
                    .into_requests(tx, &self.host)
                    .map_err(|e| PipelineError::AdapterBuild(e.to_string()))?;
                (leaves, leaf_requests)
            }
            (ResolverOutcome::Resolved(_), None) => {
                unreachable!("Resolved outcome always carries an adapter")
            }
        };
        let tx_request = request_for_tx(tx, &leaves, &leaf_requests);
        let mut requests_with_origin = Vec::new();
        for (idx, req) in leaf_requests.iter().enumerate() {
            requests_with_origin.push((req.clone(), RequestKind::Leaf { index: idx }));
        }
        requests_with_origin.push((tx_request.clone(), RequestKind::Tx));

        Ok((leaves, leaf_requests, tx_request, requests_with_origin))
    }

    pub fn evaluate_with_reservation(
        &self,
        tx: &TransactionRequest,
    ) -> Result<EvaluationOutcome, PipelineError> {
        let (leaves, leaf_requests, mut tx_request, mut requests_with_origin) =
            self.build_requests(tx)?;
        enrich_tx_request_with_window_stats(
            &mut tx_request,
            &tx.from,
            &[
                StatKey::new("swap_volume_usd_24h"),
                StatKey::new("swap_count_24h"),
            ],
            &self.host,
        );
        if let Some(last_request) = requests_with_origin.last_mut() {
            *last_request = (tx_request, RequestKind::Tx);
        }

        let verdict = self.policies.evaluate_requests(
            requests_with_origin
                .iter()
                .map(|(request, origin)| (request, origin.clone())),
        )?;

        let reservation = if !matches!(verdict, Verdict::Fail(_)) {
            self.host.stats().map(|stats| {
                let deltas = compute_swap_window_deltas(&leaves, &leaf_requests);
                if deltas.is_empty() {
                    None
                } else {
                    Some(stats.reserve(&tx.from, deltas))
                }
            })
        } else {
            None
        };

        Ok(EvaluationOutcome {
            verdict,
            reservation: reservation.flatten(),
        })
    }

    pub fn evaluate(&self, tx: &TransactionRequest) -> Result<Verdict, PipelineError> {
        let (_leaves, _leaf_requests, mut tx_request, mut requests_with_origin) =
            self.build_requests(tx)?;
        enrich_tx_request_with_window_stats(
            &mut tx_request,
            &tx.from,
            &[
                StatKey::new("swap_volume_usd_24h"),
                StatKey::new("swap_count_24h"),
            ],
            &self.host,
        );
        if let Some(last_request) = requests_with_origin.last_mut() {
            *last_request = (tx_request, RequestKind::Tx);
        }

        Ok(self.policies.evaluate_requests(
            requests_with_origin
                .iter()
                .map(|(request, origin)| (request, origin.clone())),
        )?)
    }
}
