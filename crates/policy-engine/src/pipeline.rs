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

use crate::core::{
    validate_typed_data, Action, Eip712OtherAction, OtherAction, Request, SignatureRequest,
    TransactionRequest,
};
use crate::host::stat_windows::ReservationId;
use crate::host::HostCapabilities;
use crate::lowering::{
    compute_dex_window_deltas, enrich_dex_action_base, enrich_dex_window_stats,
    enrich_signature_action, request_from_action, request_from_action_with_host,
};
use crate::policy::{PolicyEngine, PolicyError, PolicyRequest, PolicyRequestOrigin, Verdict};
use crate::registry::{
    SignatureActionAdapterRegistry, SignatureActionResolverOutcome,
    TransactionActionAdapterRegistry, TransactionResolverOutcome,
};
use thiserror::Error;

/// Pipeline execution failures.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Multiple adapters matched the transaction.
    #[error("adapter ambiguity: {0:?}")]
    Ambiguous(Vec<crate::ActionAdapterId>),
    /// The selected adapter failed to build an action.
    #[error("adapter build failed: {0}")]
    AdapterBuild(String),
    /// Request validation or lowering failed before Cedar evaluation.
    #[error("lowering failed: {0}")]
    Lowering(String),
    /// Policy evaluation failed.
    #[error("policy evaluation failed: {0}")]
    Policy(#[from] PolicyError),
}

/// Coordinates adapter resolution, lowering, and policy evaluation.
///
/// The `R: ?Sized` bound lets callers pass either a concrete registry or a
/// `dyn TransactionActionAdapterRegistry` trait object. Host capabilities are
/// passed as a small borrowed bundle to avoid `Pipeline::new` signature churn
/// as capabilities expand.
pub struct Pipeline<'a, R: TransactionActionAdapterRegistry + ?Sized> {
    /// `TransactionActionAdapter` registry used to resolve calldata.
    pub registry: &'a R,
    /// Optional signature registry used to resolve EIP-712 typed data.
    pub signature_registry: Option<&'a dyn SignatureActionAdapterRegistry>,
    /// Host capabilities used for enrichment.
    pub host: HostCapabilities<'a>,
    /// Policy engine used for evaluation.
    pub policies: &'a PolicyEngine,
}

impl<R: TransactionActionAdapterRegistry + ?Sized> std::fmt::Debug for Pipeline<'_, R> {
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

impl<'a, R: TransactionActionAdapterRegistry + ?Sized> Pipeline<'a, R> {
    /// Construct a pipeline from registry, host capabilities, and policies.
    #[must_use]
    pub const fn new(
        registry: &'a R,
        host: HostCapabilities<'a>,
        policies: &'a PolicyEngine,
    ) -> Self {
        Self {
            registry,
            signature_registry: None,
            host,
            policies,
        }
    }

    /// Attach a signature registry for EIP-712 evaluation.
    #[must_use]
    pub const fn with_signature_registry(
        mut self,
        sig_registry: &'a dyn SignatureActionAdapterRegistry,
    ) -> Self {
        self.signature_registry = Some(sig_registry);
        self
    }

    /// Build the semantic [`Action`] for a request without enrichment, lowering,
    /// or evaluation.
    ///
    /// Wraps the existing private TX and signature builders behind the unified
    /// [`Request`] surface. Used by external orchestrators (notably the Chrome
    /// extension's WASM bridge) that want to derive a [`HostFactPlan`] from
    /// the bare action and prefetch host data before evaluation.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Ambiguous`] if multiple adapters resolve, and
    /// [`PipelineError::AdapterBuild`] if the matched adapter fails to build.
    ///
    /// [`HostFactPlan`]: crate::lowering::HostFactPlan
    pub fn build_action_for(&self, request: &Request) -> Result<Action, PipelineError> {
        match request {
            Request::Tx(tx) => self.build_action_for_tx(tx),
            Request::Sig(sig) => {
                // Mirror `evaluate_sig`: validate typed data before adapter
                // dispatch so external orchestrators receive the same
                // PipelineError::Lowering boundary error they would get from a
                // full evaluation. Without this, planning succeeds for a
                // signature that evaluation later rejects.
                validate_typed_data(&sig.typed_data)
                    .map_err(|e| PipelineError::Lowering(e.to_string()))?;
                self.build_action_for_signature(sig)
            }
        }
    }

    fn build_action_for_tx(&self, tx: &TransactionRequest) -> Result<Action, PipelineError> {
        let (outcome, adapter) = self.registry.resolve_with_adapter(tx);

        match (outcome, adapter) {
            (TransactionResolverOutcome::Ambiguous(ids), _) => Err(PipelineError::Ambiguous(ids)),
            (TransactionResolverOutcome::NoMatch, _) => {
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
            (TransactionResolverOutcome::Resolved(_), Some(adapter)) => adapter
                .build_action(tx)
                .map_err(|e| PipelineError::AdapterBuild(e.to_string())),
            (TransactionResolverOutcome::Resolved(_), None) => {
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
        let mut action = self.build_action_for_tx(tx)?;
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

        let request = request_from_action(&action)?;
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

    /// Evaluate a request without creating a reservation.
    ///
    /// # Errors
    ///
    /// Returns an error when adapter resolution/building or policy evaluation
    /// fails.
    pub fn evaluate<'r, I>(&self, request: I) -> Result<Verdict, PipelineError>
    where
        I: Into<PipelineRequest<'r>>,
    {
        match request.into() {
            PipelineRequest::Tx(tx) => self.evaluate_tx(tx),
            PipelineRequest::Sig(sig) => self.evaluate_sig(sig),
        }
    }

    fn evaluate_tx(&self, tx: &TransactionRequest) -> Result<Verdict, PipelineError> {
        let mut action = self.build_action_for_tx(tx)?;

        if let Action::Dex(dex) = &mut action {
            enrich_dex_action_base(dex, &self.host);
            let deltas = compute_dex_window_deltas(dex);
            enrich_dex_window_stats(dex, &self.host, &deltas);
        }

        let request = request_from_action(&action)?;
        Ok(self.evaluate_one_request(&request)?)
    }

    fn evaluate_sig(&self, sig: &SignatureRequest) -> Result<Verdict, PipelineError> {
        validate_typed_data(&sig.typed_data).map_err(|e| PipelineError::Lowering(e.to_string()))?;
        #[cfg(test)]
        self.host.assert_signature_clock_not_default();

        let mut action = self.build_action_for_signature(sig)?;
        enrich_signature_action(&mut action, &self.host);

        let request = request_from_action_with_host(&action, &self.host)
            .map_err(|e| PipelineError::Lowering(e.to_string()))?;
        Ok(self.evaluate_one_request(&request)?)
    }

    fn build_action_for_signature(&self, sig: &SignatureRequest) -> Result<Action, PipelineError> {
        if let Some(registry) = self.signature_registry {
            match registry.resolve(sig) {
                SignatureActionResolverOutcome::Resolved(adapter) => {
                    return adapter
                        .build_action(sig)
                        .map_err(|e| PipelineError::AdapterBuild(e.to_string()));
                }
                SignatureActionResolverOutcome::NoMatch => {}
                SignatureActionResolverOutcome::Ambiguous(ids) => {
                    return Err(PipelineError::Ambiguous(ids));
                }
            }
        }

        Ok(Action::Eip712Other(Eip712OtherAction::from_request(sig)))
    }

    fn evaluate_one_request(&self, request: &PolicyRequest) -> Result<Verdict, PolicyError> {
        self.policies
            .evaluate_requests(std::iter::once((request, PolicyRequestOrigin::Action)))
    }

    fn release_reservation(&self, reservation: Option<ReservationId>) {
        if let (Some(id), Some(stats)) = (reservation, self.host.stats()) {
            stats.release(id);
        }
    }
}

/// Borrowed request accepted by [`Pipeline::evaluate`].
#[derive(Debug, Clone, Copy)]
pub enum PipelineRequest<'a> {
    /// Transaction evaluation request.
    Tx(&'a TransactionRequest),
    /// Signature evaluation request.
    Sig(&'a SignatureRequest),
}

impl<'a> From<&'a TransactionRequest> for PipelineRequest<'a> {
    fn from(value: &'a TransactionRequest) -> Self {
        Self::Tx(value)
    }
}

impl<'a> From<&'a SignatureRequest> for PipelineRequest<'a> {
    fn from(value: &'a SignatureRequest) -> Self {
        Self::Sig(value)
    }
}

impl<'a> From<&'a Request> for PipelineRequest<'a> {
    fn from(value: &'a Request) -> Self {
        match value {
            Request::Tx(tx) => Self::Tx(tx),
            Request::Sig(sig) => Self::Sig(sig),
        }
    }
}

#[cfg(test)]
mod build_action_for_tests {
    use super::*;
    use crate::core::{Address, Request, SignatureRequest, TransactionRequest};
    use crate::host::{oracle::MockOracle, HostCapabilities};
    use crate::policy::PolicyEngine;
    use crate::registry::MockTransactionActionAdapterRegistry;

    fn empty_pipeline_fixture() -> (
        MockTransactionActionAdapterRegistry,
        MockOracle,
        PolicyEngine,
    ) {
        let registry = MockTransactionActionAdapterRegistry::default();
        let oracle = MockOracle::new();
        let engine = PolicyEngine::builder()
            .build()
            .expect("empty PolicyEngine builds");
        (registry, oracle, engine)
    }

    #[test]
    fn build_action_for_tx_returns_other_when_no_adapter_matches() {
        let (registry, oracle, policies) = empty_pipeline_fixture();
        let host = HostCapabilities::new(&oracle);
        let pipeline = Pipeline::new(&registry, host, &policies);

        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x1111111111111111111111111111111111111111").unwrap(),
            to: Address::new("0x2222222222222222222222222222222222222222").unwrap(),
            value_wei: "0".into(),
            data: vec![0xde, 0xad, 0xbe, 0xef],
            gas: None,
            nonce: None,
        };
        let action = pipeline.build_action_for(&Request::Tx(tx)).unwrap();
        assert!(matches!(action, Action::Other(_)));
    }

    #[test]
    fn build_action_for_sig_returns_eip712_other_when_no_adapter_matches() {
        let (registry, oracle, policies) = empty_pipeline_fixture();
        let host = HostCapabilities::new(&oracle);
        let pipeline = Pipeline::new(&registry, host, &policies);

        let sig = SignatureRequest::test_minimal_eip712_other();
        let action = pipeline.build_action_for(&Request::Sig(sig)).unwrap();
        assert!(matches!(action, Action::Eip712Other(_)));
    }
}
