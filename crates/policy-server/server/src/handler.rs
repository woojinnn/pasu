//! The core simulation handler — load canonical state → simulate prediction.
//! Given an [`EvaluateRequest`], this loads the wallet's `state_before` via the
//! [`WalletStore`] boundary, folds each request `envelope` through
//! [`policy_transition::apply()`] + `apply_delta` to produce one delta per
//! action and a final predicted `state_after`, and returns the
//! [`EvaluateResponse`] the extension's Cedar layer consumes.
//! Important boundary: reducer deltas are *predictions* for policy evaluation,
//! not authoritative ledger facts. Canonical wallet state is updated by sync /
//! receipt / venue reconciliation, not by `evaluate`.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use policy_state::store::StoreError;
use policy_state::WalletStore;
use policy_transition::apply;
use policy_transition::error::ReducerError;
use policy_transition::helpers::delta::apply_delta;

use crate::dto::{Diagnostic, EvaluateRequest, EvaluateResponse, PolicyRequest};

/// Error surfaced by [`evaluate`].
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
/// Loads `state_before` from `store`, applies each envelope in order through
/// the reducer (one [`policy_state::StateDelta`] per action), folds those
/// deltas into an in-memory predicted `state_after`, and returns the
/// [`EvaluateResponse`].
/// This function deliberately does **not** call [`WalletStore::save`] with the
/// predicted state. A policy verdict says "this would be allowed"; it does not
/// prove the browser extension reached the wallet confirmation screen, that the
/// wallet signed, that an on-chain transaction landed, or that an off-chain
/// venue accepted the request. Authoritative sync/report reconciliation owns
/// canonical mutation.
/// # Errors
/// Returns [`HandlerError::Store`] if loading wallet state fails, or
/// [`HandlerError::Reducer`] if any action cannot be applied to the running
/// predicted state.
pub async fn evaluate(
    store: &dyn WalletStore,
    req: EvaluateRequest,
) -> Result<EvaluateResponse, HandlerError> {
    // This handler is db-agnostic: production passes the PostgreSQL-backed
    // store, while narrow unit tests can pass `InMemoryWalletStore`.
    let state_before = store.load(&req.wallet_id).await?;

    // Running state, folded forward one envelope at a time.
    let mut state = state_before.clone();
    let mut deltas = Vec::with_capacity(req.envelopes.len());

    for (i, action) in req.envelopes.iter().enumerate() {
        // The reducer is pure: it reads only `(state, action, ctx)`. The
        // per-envelope index lets the reducer disambiguate intra-batch effects.
        let ctx = req.eval_context.clone().with_action_index(i);

        // `evaluate` applies the action as supplied by the caller. Canonical
        // state freshness is handled by wallet sync/reconciliation endpoints;
        // action-scoped network refresh belongs at the HTTP layer before this
        // pure reducer boundary.

        let delta = apply(&state, action, &ctx)?;
        state = apply_delta(&state, &delta)?;
        deltas.push(delta);
    }

    // The browser extension owns policy and verdict evaluation. Server-side
    // enrichment results are intentionally empty until a typed call-spec
    // executor is introduced at the HTTP boundary.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::position::PositionKind;
    use policy_state::primitives::{Address, BlockHeight, ChainId, Decimal, Time};
    use policy_state::{RequestKind, WalletId, WalletState};
    use policy_transition::action::hyperliquid_core::{HlOrderAction, HyperliquidCoreAction};
    use policy_transition::{Action, ActionBody, ActionMeta, ActionNature, Eip712Domain};

    use crate::dto::EvaluateRequest;
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
            eval_context: policy_state::EvalContext::new(
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
}
