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
use crate::lowering::request_from_action;
use crate::oracle::Oracle;
use crate::policy::{PolicyEngine, PolicyError, Verdict};
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
/// runtime). The oracle stays Sized: `Adapter::into_request` takes
/// `&dyn Oracle`, and unsizing `&O → &dyn Oracle` requires `O: Sized`.
/// Callers wanting trait-object oracles can wrap in `Box<dyn Oracle>` and
/// pass `&*oracle_box` (gets a `&dyn Oracle`).
pub struct Pipeline<'a, R: AdapterRegistry + ?Sized, O: Oracle> {
    pub registry: &'a R,
    pub oracle: &'a O,
    pub policies: &'a PolicyEngine,
}

impl<'a, R: AdapterRegistry + ?Sized, O: Oracle> Pipeline<'a, R, O> {
    pub fn new(registry: &'a R, oracle: &'a O, policies: &'a PolicyEngine) -> Self {
        Pipeline {
            registry,
            oracle,
            policies,
        }
    }

    pub fn evaluate(&self, tx: &TransactionRequest) -> Result<Verdict, PipelineError> {
        let (outcome, adapter) = self.registry.resolve_with_adapter(tx);

        let requests = match (outcome, adapter) {
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
                vec![request_from_action(&action)]
            }
            (ResolverOutcome::Resolved(_), Some(adapter)) => adapter
                .into_requests(tx, self.oracle)
                .map_err(|e| PipelineError::AdapterBuild(e.to_string()))?,
            (ResolverOutcome::Resolved(_), None) => {
                unreachable!("Resolved outcome always carries an adapter")
            }
        };

        Ok(self.policies.evaluate_requests(&requests)?)
    }
}
