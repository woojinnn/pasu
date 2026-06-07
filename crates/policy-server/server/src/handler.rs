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

use async_trait::async_trait;
use policy_state::store::StoreError;
use policy_state::{WalletState, WalletStore, U256};
use policy_transition::apply;
use policy_transition::error::ReducerError;
use policy_transition::helpers::delta::apply_delta;
use serde_json::{json, Value};

use crate::dto::{CallSpec, Diagnostic, EvaluateRequest, EvaluateResponse, PolicyRequest};

/// A market-global price source: USD price + decimals for a `(chain, address)`
/// token, independent of any specific wallet. The price of a `(chain, contract)`
/// pair is identical across wallets, so this lets `oracle.usd_value` value a
/// swap even when the *requesting* wallet has never been synced — fixing the
/// surprise that an address-independent USD-cap policy needed the wallet
/// registered. Production backs this with the global DB
/// (`PostgresGlobalDb::latest_token_price`); unit tests use a stub.
#[async_trait]
pub trait PriceBook: Send + Sync {
    /// Global USD price (decimal string) + token decimals for `(chain, address)`,
    /// or `None` when the token's price is not known market-wide.
    async fn price(&self, chain: &str, address: &str) -> Option<PriceFact>;

    /// Global token `decimals` for `(chain, address)`, independent of price.
    /// Lets `token.normalize_to_nano` rescale with the token's REAL decimals
    /// instead of a hard-coded literal, so a token-amount cap needs no per-token
    /// gating. `None` when the token has never been synced anywhere.
    async fn decimals(&self, chain: &str, address: &str) -> Option<u8>;
}

/// Price + decimals returned by a [`PriceBook`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceFact {
    /// USD price as a decimal string (e.g. `"0.99959644"`).
    pub price_usd: String,
    /// Token decimals (e.g. `6` for USDC).
    pub decimals: u8,
}

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
    price_book: &dyn PriceBook,
    req: EvaluateRequest,
) -> Result<EvaluateResponse, HandlerError> {
    // This handler is db-agnostic: production passes the PostgreSQL-backed
    // store, while narrow unit tests can pass `InMemoryWalletStore`.
    let state_before = store.load(&req.wallet_id).await?;

    // Running state, folded forward one envelope at a time.
    //
    // A reducer rejection is NON-FATAL. Enrichment below sources its facts from
    // `state_before` (the synced price/decimals), NOT from the simulated state —
    // so a USD/nano cap still evaluates even for an action the reducer can't
    // simulate (e.g. a Uniswap v4 multicall whose pool state isn't modelled).
    // We keep the partial deltas + a diagnostic and fall through to enrichment;
    // the recording path already treats deltas as best-effort.
    let mut state = state_before.clone();
    let mut deltas = Vec::with_capacity(req.envelopes.len());
    let mut sim_diagnostics: Vec<Diagnostic> = Vec::new();

    // Diagnostic for an envelope the reducer can't simulate — stop folding but
    // keep going to enrichment.
    let sim_skip = |i: usize, err: &dyn fmt::Display| Diagnostic {
        level: "warn".to_owned(),
        message: format!(
            "simulate envelope {i} not reducible; enrichment still served from \
             synced state: {err}"
        ),
        call_id: None,
    };

    for (i, action) in req.envelopes.iter().enumerate() {
        // The reducer is pure: it reads only `(state, action, ctx)`. The
        // per-envelope index lets the reducer disambiguate intra-batch effects.
        let ctx = req.eval_context.clone().with_action_index(i);
        match apply(&state, action, &ctx) {
            Ok(delta) => match apply_delta(&state, &delta) {
                Ok(next) => {
                    state = next;
                    deltas.push(delta);
                }
                Err(err) => {
                    sim_diagnostics.push(sim_skip(i, &err));
                    break;
                }
            },
            Err(err) => {
                sim_diagnostics.push(sim_skip(i, &err));
                break;
            }
        }
    }

    // Execute the manifest-planned enrichment calls server-side, sourcing facts
    // from the wallet state we LOADED (`state_before`), independent of whether the
    // action could be simulated. `oracle.usd_value` reads the synced `price_usd`
    // on the held token, falling back to the market-global `price_book` when the
    // requesting wallet doesn't hold it — no live network call either way — so a
    // USD-cap policy's `context.custom.*Usd` field is populated even for a wallet
    // that was never registered/synced.
    let (results, mut diagnostics) =
        execute_call_specs(&state_before, &req.call_specs, price_book).await;
    diagnostics.append(&mut sim_diagnostics);

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
async fn execute_call_specs(
    state: &WalletState,
    specs: &[CallSpec],
    price_book: &dyn PriceBook,
) -> (BTreeMap<String, Value>, Vec<Diagnostic>) {
    let mut results = BTreeMap::new();
    let mut diagnostics = Vec::new();
    tracing::debug!(
        n_specs = specs.len(),
        n_synced_tokens = state.tokens.len(),
        "execute_call_specs: enrichment requested"
    );
    for spec in specs {
        tracing::debug!(
            call_id = %spec.call_id,
            method = %spec.method,
            params = %spec.params,
            "execute_call_specs: serving call"
        );
        match spec.method.as_str() {
            "oracle.usd_value" => match oracle_usd_value(state, &spec.params, price_book).await {
                Some(usd) => {
                    tracing::debug!(call_id = %spec.call_id, usd = %usd, "oracle.usd_value: OK");
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
            "token.normalize_to_nano" => match normalize_to_nano(&spec.params, price_book).await {
                Some(nano) => {
                    tracing::debug!(call_id = %spec.call_id, nano, "token.normalize_to_nano: OK");
                    results.insert(spec.call_id.clone(), json!({ "nano": nano }));
                }
                None => diagnostics.push(Diagnostic {
                    level: "warn".to_owned(),
                    message: format!(
                        "token.normalize_to_nano: no synced decimals for the requested asset \
                         (call {}) — field left unset",
                        spec.call_id
                    ),
                    call_id: Some(spec.call_id.clone()),
                }),
            },
            "intent.pending_cap_over_balance" => {
                match crate::methods::pending_cap_over_balance(state, &spec.params) {
                    Some(value) => {
                        tracing::debug!(
                            call_id = %spec.call_id,
                            "intent.pending_cap_over_balance: OK"
                        );
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "intent.pending_cap_over_balance: unparseable params or unsynced \
                             sell-token balance (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
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

/// Value an `oracle.usd_value` call: resolve the token's `(price_usd, decimals)`,
/// then compute `amount / 10^decimals × price_usd`. f64-scale (display, mirrors
/// [`TokenHolding::compute_value_usd`]) and formatted to 4 fractional digits so
/// it parses as a Cedar `decimal`.
///
/// Price + decimals come from the requesting wallet's own synced holding when it
/// holds the asset (freshest for that user); otherwise from the market-global
/// [`PriceBook`] — the price of a `(chain, contract)` pair is wallet-independent,
/// so a USD-cap policy works even for a wallet that was never registered. Returns
/// `None` when neither source knows the price, or the amount cannot be parsed.
async fn oracle_usd_value(
    state: &WalletState,
    params: &Value,
    price_book: &dyn PriceBook,
) -> Option<String> {
    let asset = params.get("asset").and_then(asset_address)?;
    let amount_raw = params.get("amount").and_then(Value::as_str)?;
    let amount = U256::from_str_radix(amount_raw.trim_start_matches("0x"), 16).ok()?;

    // Prefer this wallet's own synced holding (present AND priced); else fall
    // back to the global price book keyed by `(chain_id, asset)`.
    let from_holding = state
        .tokens
        .values()
        .find(|h| h.key.contract().map(|a| format!("{a:#x}")).as_deref() == Some(asset.as_str()))
        .and_then(|h| {
            let price: f64 = h.price_usd.as_ref()?.value.as_str().parse().ok()?;
            Some((price, h.decimals))
        });

    let (price_f, decimals): (f64, u8) = if let Some(pd) = from_holding {
        pd
    } else {
        let chain = params.get("chain_id").and_then(Value::as_str)?;
        let fact = price_book.price(chain, &asset).await?;
        (fact.price_usd.parse().ok()?, fact.decimals)
    };

    let amount_f: f64 = amount.to_string().parse().ok()?;
    let divisor = 10f64.powi(i32::from(decimals));
    if divisor <= 0.0 {
        return None;
    }
    let usd = amount_f / divisor * price_f;
    Some(format!("{usd:.4}"))
}

/// Server-side `token.normalize_to_nano`: rescale a raw token amount to
/// token-native nano (`raw × 10^(9 − decimals)`, i.e. `token_amount × 10^9`),
/// resolving the token's REAL `decimals` from the market-global [`PriceBook`] by
/// `(chain_id, asset)` instead of a hard-coded literal — so a token-amount cap
/// works for ANY token without per-token (e.g. USDC-only) gating.
///
/// Mirrors the SW's local pure handler (`NANO_SCALE = 9`, clamp to JS
/// `MAX_SAFE_INTEGER`) so a value computed here is bit-identical to one computed
/// in-process. Returns `None` when decimals are unknown, the amount can't be
/// parsed, or the rescaled value overflows JS Number range.
async fn normalize_to_nano(params: &Value, price_book: &dyn PriceBook) -> Option<i64> {
    const NANO_SCALE: u32 = 9;
    // Largest BigInt the SW can read back over JSON as a `number` without
    // precision loss (`Number.MAX_SAFE_INTEGER`, 2^53 − 1).
    let max_safe = U256::from(9_007_199_254_740_991u64);

    let asset = params.get("asset").and_then(asset_address)?;
    let chain = params.get("chain_id").and_then(Value::as_str)?;
    let amount_raw = params.get("amount").and_then(Value::as_str)?;
    let amount = U256::from_str_radix(amount_raw.trim_start_matches("0x"), 16).ok()?;

    let decimals = price_book.decimals(chain, &asset).await?;
    let dec = u32::from(decimals);

    // nano = raw × 10^(9 − decimals); for decimals > 9 it divides instead so the
    // unit stays `token_amount × 10^9` regardless of the token's own decimals.
    let nano = if dec <= NANO_SCALE {
        amount.checked_mul(U256::from(10u64).pow(U256::from(u64::from(NANO_SCALE - dec))))?
    } else {
        amount / U256::from(10u64).pow(U256::from(u64::from(dec - NANO_SCALE)))
    };

    if nano > max_safe {
        return None;
    }
    nano.to_string().parse::<i64>().ok()
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
        RequestKind, TokenHolding, TokenKey, TokenKind, TokenRef, WalletId, WalletState, U256,
    };
    use policy_transition::action::hyperliquid_core::{HlOrderAction, HyperliquidCoreAction};
    use policy_transition::action::token::{Erc20PermitAction, TokenAction};
    use policy_transition::{Action, ActionBody, ActionMeta, ActionNature, Eip712Domain};

    use crate::dto::{CallSpec, EvaluateRequest};
    use crate::store::InMemoryWalletStore;

    /// A test [`PriceBook`] returning fixed `(price, decimals)` for ANY asset.
    struct StubPriceBook(Option<PriceFact>, Option<u8>);
    #[async_trait]
    impl PriceBook for StubPriceBook {
        async fn price(&self, _chain: &str, _address: &str) -> Option<PriceFact> {
            self.0.clone()
        }
        async fn decimals(&self, _chain: &str, _address: &str) -> Option<u8> {
            self.1
        }
    }

    /// The default for holding-path tests: the global book knows nothing, so any
    /// computed value MUST have come from the wallet's own synced holding.
    fn no_price_book() -> StubPriceBook {
        StubPriceBook(None, None)
    }

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

    /// An action the reducer REJECTS: an `Erc20Permit` whose token key is Native
    /// (the reducer raises `Invariant` before touching state). Stands in for any
    /// un-simulatable action — e.g. a Uniswap v4 multicall whose pool isn't
    /// modelled — so we can assert enrichment survives a simulation failure.
    fn unreducible_action() -> Action {
        Action {
            meta: ActionMeta {
                submitted_at: Time::from_unix(1_700_000_000),
                submitter: sample_wallet_id().address,
                nature: ActionNature::OffchainSig {
                    domain: Eip712Domain {
                        name: "Permit".to_owned(),
                        version: None,
                        chain_id: None,
                        verifying_contract: None,
                        salt: None,
                    },
                    deadline: Time::from_unix(1_700_000_600),
                    nonce_key: None,
                },
            },
            body: ActionBody::Token(TokenAction::Erc20Permit(Erc20PermitAction {
                token: TokenRef {
                    key: TokenKey::Native {
                        chain: ChainId::ethereum_mainnet(),
                    },
                },
                spender: Address::from_str("0x000000000000000000000000000000000000dead").unwrap(),
                amount: U256::from(1u64),
                deadline: Time::from_unix(1_700_000_600),
                nonce: LiveField::new(
                    U256::ZERO,
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "nonce".into(),
                    },
                    Time::from_unix(1_700_000_000),
                ),
            })),
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

        let resp = evaluate(&store, &no_price_book(), empty_envelope_request())
            .await
            .unwrap();

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
        let resp = evaluate(&store, &no_price_book(), empty_envelope_request())
            .await
            .unwrap();
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

        let resp = evaluate(
            &store,
            &no_price_book(),
            request_with_envelope(hyperliquid_order_action()),
        )
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
    /// $1.0001 → "100.0100", folded into `results` under the `call_id`.
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

        let resp = evaluate(&store, &no_price_book(), req).await.unwrap();
        assert_eq!(
            resp.policy_request.results["swap-usdc-usd-cap-deny::usd"],
            serde_json::json!({ "usd": "100.0100" })
        );
    }

    /// Enrichment is served from synced state even when the action itself can't
    /// be simulated (the reducer rejects it). This is what lets a USD-cap policy
    /// evaluate a Uniswap v4 multicall, whose `/evaluate` simulation 422s.
    #[tokio::test]
    async fn enrichment_served_even_when_action_is_not_reducible() {
        let store = InMemoryWalletStore::new();
        let (state, _) = state_with_usdc_price();
        store.seed(state);

        let mut req = empty_envelope_request();
        req.envelopes.push(unreducible_action());
        req.call_specs.push(usd_call_spec(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0x5f5e100",
        ));

        let resp = evaluate(&store, &no_price_book(), req)
            .await
            .expect("a non-reducible action must NOT fail the request");

        // The USD value was still computed from the synced price.
        assert_eq!(
            resp.policy_request.results["swap-usdc-usd-cap-deny::usd"],
            serde_json::json!({ "usd": "100.0100" })
        );
        // The simulation failure is surfaced as a non-fatal diagnostic, and no
        // delta was produced for the rejected action.
        assert!(
            resp.diagnostics
                .iter()
                .any(|d| d.level == "warn" && d.message.contains("not reducible")),
            "expected a 'not reducible' diagnostic"
        );
        assert!(resp.policy_request.deltas.is_empty());
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

        let resp = evaluate(&store, &no_price_book(), req).await.unwrap();
        assert!(
            resp.policy_request.results.is_empty(),
            "no price → no result"
        );
        assert!(
            resp.diagnostics
                .iter()
                .any(|d| d.level == "warn" && d.message.contains("no synced price")),
            "miss should surface a diagnostic"
        );
    }

    /// Global fallback: a wallet that holds NOTHING still gets a USD value when
    /// the market-global price book knows the token's price + decimals. This is
    /// the fix that lets an address-independent USD-cap policy fire without the
    /// requesting wallet ever being registered/synced.
    #[tokio::test]
    async fn oracle_usd_value_falls_back_to_global_price_book() {
        let store = InMemoryWalletStore::new();
        // Empty wallet — no holdings seeded; the value can ONLY come from the book.
        let mut req = empty_envelope_request();
        // 0.06 USDC = 60_000 raw (6 decimals) = 0xea60.
        req.call_specs.push(usd_call_spec(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0xea60",
        ));
        let price_book = StubPriceBook(
            Some(PriceFact {
                price_usd: "0.9996".into(),
                decimals: 6,
            }),
            None,
        );

        let resp = evaluate(&store, &price_book, req).await.unwrap();

        // 60_000 / 1e6 × 0.9996 = 0.059976 → "0.0600" (≥ 0.05 ⇒ a USD cap denies).
        assert_eq!(
            resp.policy_request.results["swap-usdc-usd-cap-deny::usd"],
            serde_json::json!({ "usd": "0.0600" })
        );
    }

    /// `token.normalize_to_nano` is served server-side from the token's REAL
    /// global decimals (no hard-coded literal, no per-token gating): 0.06 USDC
    /// (raw `60_000`, decimals 6) → `60_000 × 10^(9−6)` = `60_000_000` nano.
    #[tokio::test]
    async fn normalize_to_nano_uses_global_decimals() {
        let store = InMemoryWalletStore::new();
        let mut req = empty_envelope_request();
        req.call_specs.push(CallSpec {
            manifest_id: "swap-intoken-cap-deny".into(),
            call_id: "swap-intoken-cap-deny::nano".into(),
            method: "token.normalize_to_nano".into(),
            params: serde_json::json!({
                "chain_id": "eip155:1",
                "asset": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "amount": "0xea60" // 60_000
            }),
            outputs: Vec::new(),
            optional: false,
        });
        // Empty wallet — decimals can ONLY come from the global book (=6, USDC).
        let price_book = StubPriceBook(None, Some(6));

        let resp = evaluate(&store, &price_book, req).await.unwrap();

        assert_eq!(
            resp.policy_request.results["swap-intoken-cap-deny::nano"],
            serde_json::json!({ "nano": 60_000_000 })
        );
    }

    /// An 18-decimals token (ETH) rescales by dividing: 1 ETH (10^18 wei) →
    /// `10^18 / 10^(18−9)` = 10^9 nano = `1 × 10^9`. Proves decimals > 9 works,
    /// which the old literal-6 path could not express.
    #[tokio::test]
    async fn normalize_to_nano_handles_18_decimals() {
        let store = InMemoryWalletStore::new();
        let mut req = empty_envelope_request();
        req.call_specs.push(CallSpec {
            manifest_id: "swap-intoken-cap-deny".into(),
            call_id: "swap-intoken-cap-deny::nano".into(),
            method: "token.normalize_to_nano".into(),
            params: serde_json::json!({
                "chain_id": "eip155:1",
                "asset": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
                "amount": "0xde0b6b3a7640000" // 1e18
            }),
            outputs: Vec::new(),
            optional: false,
        });
        let price_book = StubPriceBook(None, Some(18));

        let resp = evaluate(&store, &price_book, req).await.unwrap();

        assert_eq!(
            resp.policy_request.results["swap-intoken-cap-deny::nano"],
            serde_json::json!({ "nano": 1_000_000_000 })
        );
    }

    /// `intent.pending_cap_over_balance` served from loaded state: an 80-cap open
    /// order selling USDC + a new 30 sell exceed the 100 balance → `true`.
    #[tokio::test]
    async fn pending_cap_over_balance_served_from_state() {
        use policy_state::pending::{
            AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
        };
        use policy_state::primitives::VenueRef;
        use policy_state::StateDelta;

        let key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        };
        let holding = TokenHolding {
            key: key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(100u64)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: DataSource::UserSupplied,
        };
        let token = TokenRef { key: key.clone() };
        let pending = PendingTx {
            id: "intent:one_inch_fusion:0xopen".into(),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef::new("one_inch_fusion"),
                sell: token.clone(),
                buy: token.clone(),
                sell_max: U256::from(80u64),
                buy_min: U256::from(1u64),
                order_kind: OrderKind::Dutch,
            },
            commitment: AssetCommitment::PermitCap {
                token,
                spender: Address::ZERO,
                max_out: U256::from(80u64),
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status: PendingStatus::Active,
                valid_until: None,
                nonce: None,
                on_chain_tx: None,
                raw_status: None,
            },
            sync: DataSource::UserSupplied,
            signed_at: Time::from_unix(0),
            signature_payload: Vec::new(),
        };
        let mut state = WalletState::new(sample_wallet_id());
        state.tokens.insert(key, holding);
        state.pending = vec![pending];

        // new order sells 30 → 80 + 30 = 110 > balance 100.
        let spec = CallSpec {
            manifest_id: "ammlp-intent-cap-over-balance-warn".into(),
            call_id: "ammlp-intent-cap-over-balance-warn::pending-cap-over-balance".into(),
            method: "intent.pending_cap_over_balance".into(),
            // The exact params shape the manifest + lowering emit: the sell token
            // nested under `action.sell.key.address`, amount at `action.sellAmount`.
            params: serde_json::json!({
                "chain_id": "eip155:1",
                "owner": "0x0000000000000000000000000000000000000000",
                "action": {
                    "sell": { "key": {
                        "standard": "erc20",
                        "chain": "eip155:1",
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    } },
                    "sellAmount": "0x1e"
                }
            }),
            outputs: Vec::new(),
            optional: true,
        };

        let (results, _diag) =
            execute_call_specs(&state, std::slice::from_ref(&spec), &no_price_book()).await;
        assert_eq!(
            results.get(&spec.call_id),
            Some(&serde_json::json!({ "capSumOverBalance": true }))
        );
    }
}
