//! Pipeline orchestrator for v0.x.
//!
//! Request flow:
//! 1) Resolve adapter and build one semantic action from a transaction.
//! 2) Enrich Dex actions with aggregate host facts.
//! 3) Stamp projected Dex stat windows when available.
//! 4) Lower the action to one Cedar request and evaluate it.
//!
//! `evaluate_with_reservation` uses reserve-first semantics: it reserves projected
//! Dex window deltas before policy evaluation and then evaluates context built
//! from a snapshot where that reservation is visible. `evaluate` instead
//! projects the same window stats on demand in a single call.

use crate::core::{Action, OtherAction, TransactionRequest};
use crate::host::stat_windows::ReservationId;
use crate::host::HostCapabilities;
use crate::lowering::{
    compute_dex_window_deltas, enrich_dex_action_base, enrich_dex_window_stats, request_from_action,
};
use crate::policy::{PolicyEngine, PolicyError, PolicyRequest, RequestKind, Verdict};
use crate::registry::{AdapterRegistry, ResolverOutcome};
use thiserror::Error;

/// Pipeline execution failures.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Multiple adapters matched the transaction.
    #[error("adapter ambiguity: {0:?}")]
    Ambiguous(Vec<crate::AdapterId>),
    /// The selected adapter failed to build an action.
    #[error("adapter build failed: {0}")]
    AdapterBuild(String),
    /// Policy evaluation failed.
    #[error("policy evaluation failed: {0}")]
    Policy(#[from] PolicyError),
}

/// Coordinates adapter resolution, lowering, and policy evaluation.
///
/// The `R: ?Sized` bound lets callers pass either a concrete registry or a
/// `dyn AdapterRegistry` trait object. Host capabilities are passed as a small
/// borrowed bundle to avoid `Pipeline::new` signature churn as capabilities
/// expand.
pub struct Pipeline<'a, R: AdapterRegistry + ?Sized> {
    /// Adapter registry used to resolve calldata.
    pub registry: &'a R,
    /// Host capabilities used for enrichment.
    pub host: HostCapabilities<'a>,
    /// Policy engine used for evaluation.
    pub policies: &'a PolicyEngine,
}

impl<R: AdapterRegistry + ?Sized> std::fmt::Debug for Pipeline<'_, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pipeline").finish_non_exhaustive()
    }
}

/// Evaluation result plus an optional stat-window reservation.
#[derive(Debug)]
pub struct EvaluationOutcome {
    /// Final policy verdict.
    pub verdict: Verdict,
    /// Reservation to settle or release after signing outcome is known.
    pub reservation: Option<ReservationId>,
}

impl<'a, R: AdapterRegistry + ?Sized> Pipeline<'a, R> {
    /// Construct a pipeline from registry, host capabilities, and policies.
    #[must_use]
    pub const fn new(
        registry: &'a R,
        host: HostCapabilities<'a>,
        policies: &'a PolicyEngine,
    ) -> Self {
        Self {
            registry,
            host,
            policies,
        }
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<Action, PipelineError> {
        let (outcome, adapter) = self.registry.resolve_with_adapter(tx);

        match (outcome, adapter) {
            (ResolverOutcome::Ambiguous(ids), _) => Err(PipelineError::Ambiguous(ids)),
            (ResolverOutcome::NoMatch, _) => {
                // No adapter matched — emit `Action::Other` and let user
                // policies decide whether to allow unrecognized calls.
                Ok(Action::Other(OtherAction {
                    actor: tx.from.clone(),
                    target: tx.to.clone(),
                    selector: tx.selector_hex().unwrap_or_else(|| "0x".into()),
                    value_wei: tx.value_wei.clone(),
                    raw_calldata: format!("0x{}", hex::encode(&tx.data)),
                }))
            }
            (ResolverOutcome::Resolved(_), Some(adapter)) => adapter
                .build(tx)
                .map_err(|e| PipelineError::AdapterBuild(e.to_string())),
            (ResolverOutcome::Resolved(_), None) => {
                unreachable!("Resolved outcome always carries an adapter")
            }
        }
    }

    /// Evaluate a transaction and reserve projected stat-window deltas.
    ///
    /// # Errors
    ///
    /// Returns an error when adapter resolution/building or policy evaluation
    /// fails.
    pub fn evaluate_with_reservation(
        &self,
        tx: &TransactionRequest,
    ) -> Result<EvaluationOutcome, PipelineError> {
        let mut action = self.build_action(tx)?;
        let mut reservation = None;

        if let Action::Dex(dex) = &mut action {
            enrich_dex_action_base(dex, &self.host);
            let deltas = compute_dex_window_deltas(dex);
            if !deltas.is_empty() {
                if let Some(stats) = self.host.stats() {
                    reservation = Some(stats.reserve(&dex.actor, deltas));
                }
            }
            enrich_dex_window_stats(dex, &self.host, &[]);
        }

        let request = request_from_action(&action);
        let verdict = match self.evaluate_one_request(&request) {
            Ok(verdict) => verdict,
            Err(error) => {
                self.release_reservation(reservation.take());
                return Err(PipelineError::Policy(error));
            }
        };

        let reservation = if matches!(verdict, Verdict::Fail(_)) {
            self.release_reservation(reservation.take());
            None
        } else {
            reservation
        };

        Ok(EvaluationOutcome {
            verdict,
            reservation,
        })
    }

    /// Evaluate a transaction without creating a reservation.
    ///
    /// # Errors
    ///
    /// Returns an error when adapter resolution/building or policy evaluation
    /// fails.
    pub fn evaluate(&self, tx: &TransactionRequest) -> Result<Verdict, PipelineError> {
        let mut action = self.build_action(tx)?;

        if let Action::Dex(dex) = &mut action {
            enrich_dex_action_base(dex, &self.host);
            let deltas = compute_dex_window_deltas(dex);
            enrich_dex_window_stats(dex, &self.host, &deltas);
        }

        let request = request_from_action(&action);
        Ok(self.evaluate_one_request(&request)?)
    }

    fn evaluate_one_request(&self, request: &PolicyRequest) -> Result<Verdict, PolicyError> {
        self.policies
            .evaluate_requests(std::iter::once((request, RequestKind::Action)))
    }

    fn release_reservation(&self, reservation: Option<ReservationId>) {
        if let (Some(id), Some(stats)) = (reservation, self.host.stats()) {
            stats.release(id);
        }
    }
}
