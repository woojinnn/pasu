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
use policy_state::{WalletState, WalletStore, U256};
use policy_transition::apply;
use policy_transition::error::ReducerError;
use policy_transition::helpers::delta::apply_delta;
use serde_json::{json, Value};

use crate::dto::{CallSpec, Diagnostic, EvaluateRequest, EvaluateResponse, PolicyRequest};

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

    // Execute the manifest-planned enrichment calls server-side, sourcing facts
    // from the wallet state we just loaded/simulated. `oracle.usd_value` reads the
    // synced `price_usd` on the held token — no live network call — so a USD-cap
    // policy's `context.custom.*Usd` field is populated from canonical state.
    let (results, mut diagnostics) = execute_call_specs(&state_before, &req.call_specs);

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

/// Execute the request's enrichment call-specs against the (already-loaded)
/// wallet state. Currently serves `oracle.usd_value` from the synced `price_usd`
/// on the held token; unknown methods are surfaced as diagnostics and skipped.
/// Returns the results map keyed by `call_id` plus non-fatal diagnostics. A call
/// that cannot be served leaves its `call_id` absent from the map — the
/// extension's materialize step fail-closes a *required* missing result and
/// fail-opens an optional one, so this never fabricates a value.
fn execute_call_specs(
    state: &WalletState,
    specs: &[CallSpec],
) -> (BTreeMap<String, Value>, Vec<Diagnostic>) {
    let mut results = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for spec in specs {
        match spec.method.as_str() {
            "oracle.usd_value" => match oracle_usd_value(state, &spec.params) {
                Some(usd) => {
                    results.insert(spec.call_id.clone(), json!({ "usd": usd }));
                }
                None => diagnostics.push(Diagnostic {
                    level: "warn".to_owned(),
                    message: format!(
                        "oracle.usd_value: no synced price for the requested asset \
                         (call {}) — field left unset",
                        spec.call_id
                    ),
                    call_id: Some(spec.call_id.clone()),
                }),
            },
            other => diagnostics.push(Diagnostic {
                level: "info".to_owned(),
                message: format!(
                    "enrichment method `{other}` not served server-side (call {})",
                    spec.call_id
                ),
                call_id: Some(spec.call_id.clone()),
            }),
        }
    }
    (results, diagnostics)
}

/// Value an `oracle.usd_value` call from synced state: locate the held token by
/// the `asset` param's address, then compute `amount / 10^decimals × price_usd`.
/// f64-scale (display, mirrors [`TokenHolding::compute_value_usd`]) and formatted
/// to 4 fractional digits so it parses as a Cedar `decimal`. Returns `None` when
/// the asset is not held, has no synced price, or the amount cannot be parsed.
fn oracle_usd_value(state: &WalletState, params: &Value) -> Option<String> {
    let asset = params.get("asset").and_then(asset_address)?;
    let amount_raw = params.get("amount").and_then(Value::as_str)?;
    let amount = U256::from_str_radix(amount_raw.trim_start_matches("0x"), 16).ok()?;

    let holding = state
        .tokens
        .values()
        .find(|h| h.key.contract().map(|a| format!("{a:#x}")).as_deref() == Some(asset.as_str()))?;

    let price_f: f64 = holding.price_usd.as_ref()?.value.as_str().parse().ok()?;
    let amount_f: f64 = amount.to_string().parse().ok()?;
    let divisor = 10f64.powi(i32::from(holding.decimals));
    if divisor <= 0.0 {
        return None;
    }
    let usd = amount_f / divisor * price_f;
    Some(format!("{usd:.4}"))
}

/// Extract a lowercase hex address from an `asset` param that may be a bare
/// address string or an `AssetRef` object carrying an `address` field.
fn asset_address(v: &Value) -> Option<String> {
    let raw = match v {
        Value::String(s) => s.clone(),
        Value::Object(_) => v.get("address").and_then(Value::as_str)?.to_owned(),
        _ => return None,
    };
    Some(raw.to_lowercase())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::position::PositionKind;
    use policy_state::primitives::{Address, BlockHeight, ChainId, Decimal, Time};
    use policy_state::{
        Balance, BaseCategory, DataSource, FiatCurrency, LiveField, OracleProvider, PegTarget,
        RequestKind, TokenHolding, TokenKey, TokenKind, WalletId, WalletState, U256,
    };
    use policy_transition::action::hyperliquid_core::{HlOrderAction, HyperliquidCoreAction};
    use policy_transition::{Action, ActionBody, ActionMeta, ActionNature, Eip712Domain};

    use crate::dto::{CallSpec, EvaluateRequest};
    use crate::store::InMemoryWalletStore;

    /// A wallet holding 100 USDC on mainnet with a synced $1.0001 price — the
    /// fact `oracle.usd_value` reads to value a swap that sells USDC.
    fn state_with_usdc_price() -> (WalletState, TokenKey) {
        let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
        let key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: usdc,
        };
        let holding = TokenHolding {
            key: key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(100_000_000u64)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(LiveField::new(
                Decimal::new("1.0001"),
                DataSource::OracleFeed {
                    provider: OracleProvider::Chainlink,
                    feed_id: "USDC/USD".into(),
                },
                Time::from_unix(1_700_000_000),
            )),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "USDC/USD".into(),
            },
        };
        let mut state = WalletState::new(sample_wallet_id());
        state.tokens.insert(key.clone(), holding);
        (state, key)
    }

    fn usd_call_spec(asset: &str, amount_hex: &str) -> CallSpec {
        CallSpec {
            manifest_id: "swap-usdc-usd-cap-deny".into(),
            call_id: "swap-usdc-usd-cap-deny::usd".into(),
            method: "oracle.usd_value".into(),
            params: serde_json::json!({
                "chain_id": "eip155:1",
                "asset": asset,
                "amount": amount_hex
            }),
            outputs: Vec::new(),
            optional: false,
        }
    }

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

    /// `oracle.usd_value` is served from the synced holding price: 100 USDC at
    /// $1.0001 → "100.0100", folded into `results` under the call_id.
    #[tokio::test]
    async fn oracle_usd_value_is_served_from_synced_price() {
        let store = InMemoryWalletStore::new();
        let (state, _) = state_with_usdc_price();
        store.seed(state);

        let mut req = empty_envelope_request();
        // 100 USDC = 100_000_000 raw (6 decimals) = 0x5f5e100.
        req.call_specs.push(usd_call_spec(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0x5f5e100",
        ));

        let resp = evaluate(&store, req).await.unwrap();
        assert_eq!(
            resp.policy_request.results["swap-usdc-usd-cap-deny::usd"],
            serde_json::json!({ "usd": "100.0100" })
        );
    }

    /// A token the wallet does not hold (no synced price) yields no result — the
    /// executor never fabricates a value; a diagnostic records the miss.
    #[tokio::test]
    async fn oracle_usd_value_skips_unpriced_asset() {
        let store = InMemoryWalletStore::new();
        let (state, _) = state_with_usdc_price();
        store.seed(state);

        let mut req = empty_envelope_request();
        req.call_specs.push(usd_call_spec(
            "0x1111111111111111111111111111111111111111", // not held
            "0x5f5e100",
        ));

        let resp = evaluate(&store, req).await.unwrap();
        assert!(resp.policy_request.results.is_empty(), "no price → no result");
        assert!(
            resp.diagnostics
                .iter()
                .any(|d| d.level == "warn" && d.message.contains("no synced price")),
            "miss should surface a diagnostic"
        );
    }
}
