//! The core simulation handler — load canonical state → simulate prediction.
//!
//! Given an [`EvaluateRequest`], this loads the wallet's `state_before` via the
//! [`WalletStore`] boundary, folds each request `envelope` through
//! [`simulation_reducer::apply`] + `apply_delta` to produce one delta per
//! action and a final predicted `state_after`, and returns the
//! [`EvaluateResponse`] the extension's Cedar layer consumes.
//!
//! Important boundary: reducer deltas are *predictions* for policy evaluation,
//! not authoritative ledger facts. Canonical wallet state is updated by sync /
//! receipt / venue reconciliation, not by `evaluate`.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use simulation_reducer::apply;
use simulation_reducer::error::ReducerError;
use simulation_reducer::helpers::delta::apply_delta;
use simulation_state::store::StoreError;
use simulation_state::WalletStore;

use crate::dto::{
    Diagnostic, EvaluateRequest, EvaluateResponse, ExecutionReportOutcome, ExecutionReportRequest,
    ExecutionReportResponse, PolicyRequest,
};
use crate::store::ExecutionReportStore;

/// Error surfaced by [`evaluate`].
///
/// `Reducer` is a *client* error (the action could not be applied to the given
/// state — map to `422 Unprocessable Entity`); `Store` is a *server* error (the
/// persistence layer failed — map to `500 Internal Server Error`).
#[derive(Debug)]
pub enum HandlerError {
    /// A reducer rejected an action (invalid for the current state).
    Reducer(ReducerError),
    /// The wallet store failed to load or save state.
    Store(StoreError),
}

impl fmt::Display for HandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reducer(e) => write!(f, "reducer error: {e}"),
            Self::Store(e) => write!(f, "store error: {e}"),
        }
    }
}

impl Error for HandlerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reducer(e) => Some(e),
            Self::Store(e) => Some(e),
        }
    }
}

impl From<ReducerError> for HandlerError {
    fn from(e: ReducerError) -> Self {
        Self::Reducer(e)
    }
}

impl From<StoreError> for HandlerError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

/// Simulates the request's action envelopes over the wallet's canonical state.
///
/// Loads `state_before` from `store`, applies each envelope in order through
/// the reducer (one [`simulation_state::StateDelta`] per action), folds those
/// deltas into an in-memory predicted `state_after`, and returns the
/// [`EvaluateResponse`].
///
/// This function deliberately does **not** call [`WalletStore::save`] with the
/// predicted state. A policy verdict says "this would be allowed"; it does not
/// prove the browser extension reached the wallet confirmation screen, that the
/// wallet signed, that an on-chain transaction landed, or that an off-chain
/// venue accepted the request. Authoritative sync/report reconciliation owns
/// canonical mutation.
///
/// # Errors
///
/// Returns [`HandlerError::Store`] if loading wallet state fails, or
/// [`HandlerError::Reducer`] if any action cannot be applied to the running
/// predicted state.
pub async fn evaluate(
    store: &dyn WalletStore,
    req: EvaluateRequest,
) -> Result<EvaluateResponse, HandlerError> {
    // TODO(prep): production `WalletStore`. This handler is db-agnostic — it
    // takes `&dyn WalletStore`. In production that trait object is the db
    // owner's SQLite impl from `simulation-db`; today `main` wires the dev/test
    // `InMemoryWalletStore`.
    let state_before = store.load(&req.wallet_id).await?;

    // Running state, folded forward one envelope at a time.
    let mut state = state_before.clone();
    let mut deltas = Vec::with_capacity(req.envelopes.len());

    for (i, action) in req.envelopes.iter().enumerate() {
        // The reducer is pure: it reads only `(state, action, ctx)`. The
        // per-envelope index lets the reducer disambiguate intra-batch effects.
        let ctx = req.eval_context.clone().with_envelope_index(i);

        // TODO(prep): live-input refresh. Once the sync orchestrator + RPC
        // config are wired, run
        //   `simulation_sync::Orchestrator::refresh_action(&mut action, &state, now)`
        // HERE — BEFORE `reducer::apply` — so each action's `live_inputs`
        // (prices/oracle values) are fetched against the *current* running
        // `state` and clock. That step does network IO, so it stays out until
        // the orchestrator + RpcConfig are injected into `AppState`. For now the
        // action's `live_inputs` are used as-supplied by the caller.

        let delta = apply(&state, action, &ctx)?;
        state = apply_delta(&state, &delta)?;
        deltas.push(delta);
    }

    // TODO(prep): enrichment-call execution. `req.call_specs` (the manifest's
    // planned enrichment calls) must be dispatched here to populate
    // `PolicyRequest::results` keyed by `CallSpec::call_id` — the Rust
    // equivalent of the Node.js policy-rpc host-capabilities / method-dispatch
    // layer. That executor (method registry + per-method enrichment) does not
    // exist in Rust yet, so `results` is empty for now and `optional` call
    // failures are not yet surfaced as diagnostics.
    let results = BTreeMap::new();

    let note = if req.envelopes.is_empty() {
        "simulated 0 envelopes (state echoed)".to_owned()
    } else {
        format!("simulated {} envelope(s)", req.envelopes.len())
    };

    Ok(EvaluateResponse {
        policy_request: PolicyRequest {
            actions: req.envelopes,
            state_before,
            deltas,
            state_after: state,
            results,
        },
        diagnostics: vec![Diagnostic {
            level: "info".to_owned(),
            message: note,
            call_id: None,
        }],
    })
}

/// Records a post-policy execution lifecycle report.
///
/// The report endpoint is deliberately *not* a state writer. It records facts
/// such as "wallet signed", "transaction submitted", or "Hyperliquid accepted
/// this order request" so a reconciliation loop can later compare them with
/// authoritative chain receipts or venue snapshots. Until that reconciliation
/// happens, canonical [`simulation_state::WalletState`] remains unchanged.
///
/// This matters for browser-extension correctness: a policy allow verdict does
/// not prove that the wallet UI was shown, the user signed, a transaction
/// landed, or a venue accepted an off-chain request.
///
/// # Errors
///
/// Returns [`HandlerError::Store`] if the report store cannot record the event.
pub async fn report_execution(
    store: &dyn ExecutionReportStore,
    req: ExecutionReportRequest,
) -> Result<ExecutionReportResponse, HandlerError> {
    let message = execution_report_message(&req.outcome).to_owned();
    store.record_execution_report(req).await?;

    Ok(ExecutionReportResponse {
        accepted: true,
        canonical_state_updated: false,
        diagnostics: vec![Diagnostic {
            level: "info".to_owned(),
            message,
            call_id: None,
        }],
    })
}

#[must_use]
fn execution_report_message(outcome: &ExecutionReportOutcome) -> &'static str {
    match outcome {
        ExecutionReportOutcome::WalletRejected { .. } => {
            "wallet rejection recorded; no canonical state update"
        }
        ExecutionReportOutcome::WalletSigned { .. } => {
            "wallet signature recorded; canonical state waits for submission and sync"
        }
        ExecutionReportOutcome::OnchainSubmitted { .. } => {
            "on-chain submission recorded; canonical state waits for receipt sync"
        }
        ExecutionReportOutcome::OnchainConfirmed { .. } => {
            "on-chain confirmation recorded; canonical state waits for receipt reconciliation"
        }
        ExecutionReportOutcome::VenueSubmitted { .. } => {
            "venue submission recorded; canonical state waits for venue sync"
        }
        ExecutionReportOutcome::VenueAccepted { .. } => {
            "venue acceptance recorded; canonical state waits for venue sync"
        }
        ExecutionReportOutcome::VenueRejected { .. } => {
            "venue rejection recorded; no canonical state update"
        }
        ExecutionReportOutcome::Failed { .. } => {
            "execution failure recorded; no canonical state update"
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_reducer::action::hyperliquid_core::{HlOrderAction, HyperliquidCoreAction};
    use simulation_reducer::{Action, ActionBody, ActionMeta, ActionNature, Eip712Domain};
    use simulation_state::position::PositionKind;
    use simulation_state::primitives::{Address, BlockHeight, ChainId, Decimal, Time};
    use simulation_state::{RequestKind, WalletId, WalletState};

    use crate::dto::{EvaluateRequest, ExecutionReportOutcome, ExecutionReportRequest};
    use crate::store::InMemoryWalletStore;

    fn sample_wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [ChainId::ethereum_mainnet()],
        )
    }

    /// A `WalletState` that is *not* bit-identical to the empty default — so the
    /// load→echo→save round-trip is observable.
    fn non_trivial_state() -> WalletState {
        let mut state = WalletState::new(sample_wallet_id());
        state.block_heights.insert(
            ChainId::ethereum_mainnet(),
            BlockHeight {
                number: 19_000_000,
                time: 1_700_000_000,
            },
        );
        state
    }

    fn empty_envelope_request() -> EvaluateRequest {
        EvaluateRequest {
            wallet_id: sample_wallet_id(),
            envelopes: Vec::new(),
            eval_context: simulation_state::EvalContext::new(
                ChainId::ethereum_mainnet(),
                Time::from_unix(1_700_000_000),
                RequestKind::Transaction,
            ),
            call_specs: Vec::new(),
        }
    }

    fn hyperliquid_order_action() -> Action {
        Action {
            meta: ActionMeta {
                submitted_at: Time::from_unix(1_700_000_000),
                submitter: sample_wallet_id().address,
                nature: ActionNature::OffchainSig {
                    domain: Eip712Domain {
                        name: "Hyperliquid".to_owned(),
                        version: None,
                        chain_id: None,
                        verifying_contract: None,
                        salt: None,
                    },
                    deadline: Time::from_unix(1_700_000_600),
                    nonce_key: None,
                },
            },
            body: ActionBody::HyperliquidCore(HyperliquidCoreAction::Order(HlOrderAction {
                asset_index: 0,
                symbol: Some("BTC".to_owned()),
                is_buy: true,
                price: Decimal::new("60000"),
                size: Decimal::new("0.1"),
                reduce_only: false,
                tif: "gtc".to_owned(),
            })),
        }
    }

    fn request_with_envelope(action: Action) -> EvaluateRequest {
        let mut req = empty_envelope_request();
        req.envelopes.push(action);
        req
    }

    /// load → echo plumbing: a seeded wallet with empty `envelopes` returns its
    /// state unchanged, with no deltas and no results.
    #[tokio::test]
    async fn empty_envelopes_echo_seeded_state() {
        let store = InMemoryWalletStore::new();
        let seeded = non_trivial_state();
        store.seed(seeded.clone());

        let resp = evaluate(&store, empty_envelope_request()).await.unwrap();

        assert_eq!(resp.policy_request.state_before, seeded);
        assert_eq!(resp.policy_request.state_after, seeded);
        assert!(resp.policy_request.deltas.is_empty());
        assert!(resp.policy_request.results.is_empty());

        // `evaluate` leaves canonical state unchanged; sync/report
        // reconciliation is the only writer of durable wallet state.
        assert_eq!(store.load(&sample_wallet_id()).await.unwrap(), seeded);
    }

    /// First-seen behavior: an unseeded wallet loads as an empty `WalletState`
    /// rather than erroring.
    #[tokio::test]
    async fn unseeded_wallet_loads_empty_state() {
        let store = InMemoryWalletStore::new();
        let id = sample_wallet_id();

        let loaded = store.load(&id).await.unwrap();
        assert_eq!(loaded, WalletState::new(id.clone()));

        // And the handler echoes that empty state for an empty request.
        let resp = evaluate(&store, empty_envelope_request()).await.unwrap();
        assert_eq!(resp.policy_request.state_before, WalletState::new(id));
        assert!(resp.policy_request.deltas.is_empty());
    }

    /// `evaluate` is allowed to fold deltas into an in-memory predicted
    /// `state_after`, but it must not persist that prediction as canonical
    /// wallet state. The real ledger/venue sync path owns canonical mutation.
    #[tokio::test]
    async fn evaluate_returns_predicted_state_without_persisting_it() {
        let store = InMemoryWalletStore::new();
        let seeded = non_trivial_state();
        store.seed(seeded.clone());

        let resp = evaluate(&store, request_with_envelope(hyperliquid_order_action()))
            .await
            .unwrap();

        assert_eq!(resp.policy_request.state_before, seeded);
        assert_eq!(resp.policy_request.deltas.len(), 1);
        assert_eq!(resp.policy_request.state_after.positions.len(), 1);
        match &resp.policy_request.state_after.positions[0].kind {
            PositionKind::HyperliquidAccount(account) => {
                assert_eq!(account.open_orders.len(), 1);
                assert_eq!(account.open_orders[0].symbol.as_deref(), Some("BTC"));
            }
            other => panic!("expected Hyperliquid account prediction, got {other:?}"),
        }

        assert_eq!(
            store.load(&sample_wallet_id()).await.unwrap(),
            seeded,
            "canonical state must wait for authoritative sync/report reconciliation"
        );
    }

    /// Hyperliquid CORE orders may be sent with an already-authorized venue
    /// agent key, so the extension can report venue acceptance without any
    /// MetaMask-style wallet signature callback. The server records that
    /// lifecycle event but still leaves canonical state to venue sync.
    #[tokio::test]
    async fn execution_report_accepts_venue_flow_without_wallet_signature() {
        let store = InMemoryWalletStore::new();
        let report = ExecutionReportRequest {
            wallet_id: Some(sample_wallet_id()),
            evaluation_id: Some("hl-eval-1".to_owned()),
            action_index: Some(0),
            outcome: ExecutionReportOutcome::VenueAccepted {
                venue: "hyperliquid".to_owned(),
                venue_order_id: Some("987654321".to_owned()),
                client_order_id: None,
            },
            metadata: BTreeMap::new(),
        };

        let resp = report_execution(&store, report.clone()).await.unwrap();

        assert!(resp.accepted);
        assert!(!resp.canonical_state_updated);
        assert!(resp
            .diagnostics
            .iter()
            .any(|d| d.message.contains("venue sync")));
        assert_eq!(store.execution_reports(), vec![report]);
    }
}
