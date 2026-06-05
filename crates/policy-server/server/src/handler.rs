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

    // Enrichment-call execution: dispatch each planned `call_spec` against the
    // simulated `state_after` (the *state/reducer* facts this sim-server owns;
    // oracle/external facts belong to the separate policy-rpc host). Each result
    // is keyed by `call_id` for the extension's `evaluate_action_v2_json`
    // materialize step. A failed call is surfaced as a diagnostic and simply
    // absent from `results` — the extension's materialize layer then applies the
    // v2 fail-open contract (optional → skip; required → SystemFail), so the
    // server stays fail-open and never decides the verdict. (No store.save here:
    // evaluate predicts state_after; canonical mutation belongs to sync, per fn doc.)
    let mut results = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let fact_ctx = crate::facts::FactCtx { state: &state };
    for spec in &req.call_specs {
        match crate::facts::dispatch(&spec.method, &spec.params, &fact_ctx) {
            Ok(value) => {
                results.insert(spec.call_id.clone(), value);
            }
            Err(err) => {
                diagnostics.push(Diagnostic {
                    level: if spec.optional { "warn" } else { "error" }.to_owned(),
                    message: format!("enrichment call `{}` failed: {err}", spec.call_id),
                    call_id: Some(spec.call_id.clone()),
                });
            }
        }
    }

    let note = if req.envelopes.is_empty() {
        "simulated 0 envelopes (state echoed)".to_owned()
    } else {
        format!("simulated {} envelope(s)", req.envelopes.len())
    };
    diagnostics.push(Diagnostic {
        level: "info".to_owned(),
        message: note,
        call_id: None,
    });

    Ok(EvaluateResponse {
        policy_request: PolicyRequest {
            actions: req.envelopes,
            state_before,
            deltas,
            state_after: state,
            results,
        },
        diagnostics,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::position::PositionKind;
    use policy_state::primitives::{Address, BlockHeight, ChainId, Decimal, Time, U256};
    use policy_state::{RequestKind, WalletId, WalletState};
    use policy_transition::action::hyperliquid_core::{HlOrderAction, HyperliquidCoreAction};
    use policy_transition::{Action, ActionBody, ActionMeta, ActionNature, Eip712Domain};

    use crate::dto::{CallSpec, EvaluateRequest};
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

    const GEN01_TOKEN: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const GEN01_SPENDER: &str = "0x00000000000000000000000000000000deadbeef";

    /// Seed a wallet holding the token with an UNLIMITED ERC20 approval to a
    /// spender, then `/evaluate` with the GEN-01 `approval-shape` `call_spec` and
    /// confirm the executed result reports `isUnlimited == true` keyed by
    /// `call_id`. This exercises the full handler enrichment path (simulate →
    /// dispatch facts → results) the route serves.
    #[tokio::test]
    async fn evaluate_executes_gen01_approval_fact_against_seeded_state() {
        use policy_state::approval::AllowanceSpec;

        let store = InMemoryWalletStore::new();
        let mut state = WalletState::new(sample_wallet_id());
        let chain = ChainId::ethereum_mainnet();
        let token = Address::from_str(GEN01_TOKEN).unwrap();
        state.approvals.erc20.insert(
            (chain.clone(), token),
            [(
                Address::from_str(GEN01_SPENDER).unwrap(),
                AllowanceSpec::unlimited(Time::from_unix(1_700_000_000)),
            )]
            .into_iter()
            .collect(),
        );
        store.seed(state);

        let max_hex = format!("{:#x}", U256::MAX);
        let req = EvaluateRequest {
            wallet_id: sample_wallet_id(),
            envelopes: Vec::new(),
            eval_context: policy_state::EvalContext::new(
                chain.clone(),
                Time::from_unix(1_700_000_000),
                RequestKind::Transaction,
            ),
            call_specs: vec![CallSpec {
                manifest_id: "unlimited-approval-deny".to_owned(),
                call_id: "approval-shape".to_owned(),
                method: "approval.unlimited_over_balance".to_owned(),
                params: serde_json::json!({
                    "chain_id": chain.to_string(),
                    "owner": "0x000000000000000000000000000000000000a01c",
                    "token": { "key": { "standard": "erc20", "chain": chain.to_string(), "address": GEN01_TOKEN } },
                    "spender": GEN01_SPENDER,
                    "amount": max_hex,
                }),
                outputs: Vec::new(),
                optional: false,
            }],
        };

        let resp = evaluate(&store, req).await.unwrap();
        let result = resp
            .policy_request
            .results
            .get("approval-shape")
            .expect("approval-shape result present");
        assert_eq!(result["isUnlimited"], serde_json::json!(true));
        // The over-balance score is a 4-dp decimal string.
        assert!(result["amountOverBalance"].is_string());
    }

    /// An UNKNOWN enrichment method does not fail the request: the result is
    /// absent and the failure is surfaced as a diagnostic (fail-open — the
    /// extension's materialize layer decides required-vs-optional).
    #[tokio::test]
    async fn evaluate_surfaces_unknown_method_as_diagnostic() {
        let store = InMemoryWalletStore::new();
        store.seed(WalletState::new(sample_wallet_id()));

        let chain = ChainId::ethereum_mainnet();
        let req = EvaluateRequest {
            wallet_id: sample_wallet_id(),
            envelopes: Vec::new(),
            eval_context: policy_state::EvalContext::new(
                chain,
                Time::from_unix(1_700_000_000),
                RequestKind::Transaction,
            ),
            call_specs: vec![CallSpec {
                manifest_id: "x".to_owned(),
                call_id: "x::oracle".to_owned(),
                method: "oracle.usd_value".to_owned(),
                params: serde_json::json!({}),
                outputs: Vec::new(),
                optional: true,
            }],
        };

        let resp = evaluate(&store, req).await.unwrap();
        assert!(resp.policy_request.results.is_empty());
        assert!(
            resp.diagnostics
                .iter()
                .any(|d| d.call_id.as_deref() == Some("x::oracle") && d.level == "warn"),
            "expected a warn diagnostic for the unknown optional call"
        );
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
