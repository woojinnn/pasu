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

/// Screens an address against an on-chain sanctions oracle (Chainalysis
/// `isSanctioned(address)` — Ethereum mainnet `0x40C5…c8fb`), independent of any
/// wallet. `Some(true)` = on the list, `Some(false)` = explicitly clean, `None` =
/// could not screen (RPC error / unconfigured / unsupported chain). A `None`
/// leaves the result absent — fail-open for the optional `address.sanctions`
/// call, fail-closed for a required one — never fabricating a verdict. The v1
/// production oracle is mainnet-only (the `EigenLayer` chain); `chain_id` is
/// advisory.
#[async_trait]
pub trait SanctionsScreen: Send + Sync {
    /// `Some(bool)` sanctioned-or-clean for `address` (0x-hex), `None` when the
    /// oracle could not answer.
    async fn is_sanctioned(&self, chain_id: i64, address: &str) -> Option<bool>;
}

/// Resolves an NFT collection's market floor price in ETH, for the
/// `marketplace.sign_order_proceeds_floor` enrichment (Seaport below-floor drain
/// shield). The production impl (`AlchemyFloorOracle`, app.rs) calls Alchemy's
/// `getFloorPrice` (which reports the floor in ETH); the method then converts
/// ETH→USD via the market-global WETH price. An unknown / unpriceable /
/// off-mainnet collection returns `None` so the optional call fail-opens (the
/// below-floor policy stays dormant), never a fabricated floor. Mirrors
/// [`SanctionsScreen`].
#[async_trait]
pub trait NftFloorOracle: Send + Sync {
    /// Floor price of NFT `collection` (a `0x` contract address, lowercase) on
    /// `chain` (CAIP-2, e.g. `eip155:1`) in **ETH**, or `None` when unknown.
    async fn floor_eth(&self, chain: &str, collection: &str) -> Option<f64>;
}

/// A floor oracle that knows nothing (always `None`) — the safe default when no
/// floor source is configured: the below-floor policy stays dormant.
pub struct NoFloorOracle;
#[async_trait]
impl NftFloorOracle for NoFloorOracle {
    async fn floor_eth(&self, _chain: &str, _collection: &str) -> Option<f64> {
        None
    }
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
    sanctions: &dyn SanctionsScreen,
    floor: &dyn NftFloorOracle,
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
        execute_call_specs(&state_before, &req.call_specs, price_book, sanctions, floor).await;
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
    sanctions: &dyn SanctionsScreen,
    floor: &dyn NftFloorOracle,
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
            "bridge.value_loss_pct" => {
                match bridge_value_loss_pct(state, &spec.params, price_book).await {
                    Some(value) => {
                        tracing::debug!(call_id = %spec.call_id, "bridge.value_loss_pct: OK");
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "bridge.value_loss_pct: unpriced leg / unparseable amount / zero input \
                             (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
            "oracle.steth_peg_status_bps" => {
                match oracle_steth_peg_status_bps(&spec.params, price_book).await {
                    Some(value) => {
                        tracing::debug!(call_id = %spec.call_id, "oracle.steth_peg_status_bps: OK");
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "oracle.steth_peg_status_bps: stETH/ETH price unavailable \
                             (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
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
            "intent.near_duplicate_pending" => {
                match crate::methods::near_duplicate_pending(state, &spec.params) {
                    Some(value) => {
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "intent.near_duplicate_pending: unparseable action params \
                             (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
            "intent.validity_horizon_sec" => {
                match crate::methods::validity_horizon_sec(state, &spec.params) {
                    Some(value) => {
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "intent.validity_horizon_sec: missing valid_until param \
                             (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
            "perp.equity_drawdown_bps" => {
                match crate::methods::equity_drawdown_bps(state, &spec.params) {
                    Some(value) => {
                        tracing::debug!(
                            call_id = %spec.call_id,
                            "perp.equity_drawdown_bps: OK"
                        );
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "perp.equity_drawdown_bps: no synced HL account / no equity \
                             baseline yet (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
            "perp.session_fill_stats" => {
                match crate::methods::session_fill_stats(state, &spec.params) {
                    Some(value) => {
                        tracing::debug!(
                            call_id = %spec.call_id,
                            "perp.session_fill_stats: OK"
                        );
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "perp.session_fill_stats: no synced HL account / empty fill \
                             window (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
            "address.sanctions" => match address_sanctions(&spec.params, sanctions).await {
                Some(value) => {
                    tracing::debug!(call_id = %spec.call_id, "address.sanctions: OK");
                    results.insert(spec.call_id.clone(), value);
                }
                None => diagnostics.push(Diagnostic {
                    level: "warn".to_owned(),
                    message: format!(
                        "address.sanctions: missing address or oracle unavailable \
                         (call {}) — field left unset",
                        spec.call_id
                    ),
                    call_id: Some(spec.call_id.clone()),
                }),
            },
            "marketplace.sign_order_proceeds_floor" => {
                match sign_order_proceeds_floor(&spec.params, price_book, floor).await {
                    Some(value) => {
                        tracing::debug!(call_id = %spec.call_id, "marketplace.sign_order_proceeds_floor: OK");
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "marketplace.sign_order_proceeds_floor: floor unavailable / \
                             unpriceable collection (call {}) — field left unset",
                            spec.call_id
                        ),
                        call_id: Some(spec.call_id.clone()),
                    }),
                }
            }
            "marketplace.fulfill_overpay_vs_floor" => {
                match fulfill_overpay_vs_floor(&spec.params, price_book, floor).await {
                    Some(value) => {
                        tracing::debug!(call_id = %spec.call_id, "marketplace.fulfill_overpay_vs_floor: OK");
                        results.insert(spec.call_id.clone(), value);
                    }
                    None => diagnostics.push(Diagnostic {
                        level: "warn".to_owned(),
                        message: format!(
                            "marketplace.fulfill_overpay_vs_floor: floor unavailable / \
                             unpriceable collection (call {}) — field left unset",
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

/// USD value (f64) of `amount` raw units of `asset` on `chain` — the requesting
/// wallet's own synced holding price when it holds the asset (freshest for that
/// user), else the market-global [`PriceBook`] keyed by `(chain, asset)`. The core
/// valuation shared with [`oracle_usd_value`]; `None` when neither source prices
/// the asset or the amount cannot be parsed.
async fn token_amount_usd(
    state: &WalletState,
    chain: &str,
    asset: &str,
    amount: U256,
    price_book: &dyn PriceBook,
) -> Option<f64> {
    let from_holding = state
        .tokens
        .values()
        .find(|h| h.key.contract().map(|a| format!("{a:#x}")).as_deref() == Some(asset))
        .and_then(|h| {
            let price: f64 = h.price_usd.as_ref()?.value.as_str().parse().ok()?;
            Some((price, h.decimals))
        });
    let (price_f, decimals): (f64, u8) = if let Some(pd) = from_holding {
        pd
    } else {
        let fact = price_book.price(chain, asset).await?;
        (fact.price_usd.parse().ok()?, fact.decimals)
    };
    let amount_f: f64 = amount.to_string().parse().ok()?;
    let divisor = 10f64.powi(i32::from(decimals));
    if divisor <= 0.0 {
        return None;
    }
    Some(amount_f / divisor * price_f)
}

/// Server-side `bridge.value_loss_pct` (Bridge preset P3 — output-value-loss
/// shield): the implied % value loss of a cross-chain bridge —
/// `(inputUsd − outputUsd) / inputUsd × 100` — where `inputUsd` values the src
/// token sent (on its chain) and `outputUsd` values the dst token delivered (on the
/// destination chain). Catches an abnormal fee spread / skim (a frontend that sets
/// `outputAmount` absurdly low so a relayer/LP pockets the difference) that the
/// absolute USD *size* cap does not. Same price source as [`oracle_usd_value`],
/// valued on each token's own chain.
///
/// Returns `{ loss_pct }` (4dp decimal string, clamped to `[0, 100]`). `None`
/// (→ field omitted → policy dormant, fail-open) when EITHER leg is unpriced, an
/// amount is unparseable, or `inputUsd` is zero — never warns on a bridge it cannot
/// value.
async fn bridge_value_loss_pct(
    state: &WalletState,
    params: &Value,
    price_book: &dyn PriceBook,
) -> Option<Value> {
    let src_chain = params.get("src_chain_id").and_then(Value::as_str)?;
    let src_asset = params.get("src_asset").and_then(asset_address)?;
    let input_raw = params.get("input_amount").and_then(Value::as_str)?;
    let input_amount = U256::from_str_radix(input_raw.trim_start_matches("0x"), 16).ok()?;

    let dst_chain = params.get("dst_chain_id").and_then(Value::as_str)?;
    let dst_asset = params.get("dst_asset").and_then(asset_address)?;
    let output_raw = params.get("output_amount").and_then(Value::as_str)?;
    let output_amount = U256::from_str_radix(output_raw.trim_start_matches("0x"), 16).ok()?;

    let input_usd =
        token_amount_usd(state, src_chain, &src_asset, input_amount, price_book).await?;
    let output_usd =
        token_amount_usd(state, dst_chain, &dst_asset, output_amount, price_book).await?;

    if input_usd <= 0.0 {
        return None; // cannot express a % of zero input → dormant
    }
    let loss = (1.0 - output_usd / input_usd).max(0.0) * 100.0;
    Some(json!({ "loss_pct": format!("{loss:.4}") }))
}

/// Value a single lowered Seaport `MarketItem` leg in USD via the market-global
/// price book. `native` legs price against the canonical WETH proxy (ETH has no
/// `(chain, address)` token); `erc20` legs price against `token`. NFT legs have no
/// fungible price here (floor is resolved separately) → `None`. Dutch-auction legs
/// (`startAmount != endAmount`) are valued at `startAmount` (the maximum
/// realizable), which is conservative against false-positives. Returns `None` when
/// the price is unknown / the amount unparseable. Mirrors [`oracle_usd_value`].
async fn value_leg_usd(chain: &str, item: &Value, price_book: &dyn PriceBook) -> Option<f64> {
    /// Canonical WETH (mainnet) — the ETH price proxy (mirrors `oracle_steth_peg_status_bps`).
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

    let kind = item.get("kind").and_then(Value::as_str)?;
    let amount_raw = item.get("startAmount").and_then(Value::as_str)?;
    let amount = U256::from_str_radix(amount_raw.trim_start_matches("0x"), 16).ok()?;
    let amount_f: f64 = amount.to_string().parse().ok()?;

    let (asset, fallback_decimals): (String, u8) = match kind {
        "native" => (WETH.to_owned(), 18),
        "erc20" => (item.get("token").and_then(Value::as_str)?.to_owned(), 18),
        _ => return None, // erc721/erc1155(_criteria) have no fungible price here
    };
    let fact = price_book.price(chain, &asset).await?;
    let price_f: f64 = fact.price_usd.parse().ok()?;
    let decimals = if kind == "erc20" {
        fact.decimals
    } else {
        fallback_decimals
    };
    let divisor = 10f64.powi(i32::from(decimals));
    if divisor <= 0.0 {
        return None;
    }
    Some(amount_f / divisor * price_f)
}

/// Market-global USD price of 1 ETH, via the canonical WETH proxy (native ETH is
/// not a `(chain, address)` token). `None` when the price is unknown / unparseable.
async fn eth_price_usd(chain: &str, price_book: &dyn PriceBook) -> Option<f64> {
    /// Canonical WETH (mainnet) — the ETH price proxy.
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
    price_book.price(chain, WETH).await?.price_usd.parse().ok()
}

/// Server-side `marketplace.sign_order_proceeds_floor` (Seaport preset P1 —
/// below-floor drain shield): compares the signer's USD proceeds against the
/// offered NFT collection(s)' USD floor and reports how far below floor the sale
/// is, in basis points. The two static `SignOrder` flatteners catch zero-proceeds
/// / giveaway drains; this catches the dust case — the signer IS paid, but for a
/// tiny fraction of value (e.g. 0.01 ETH for a floor-10-ETH NFT). Floor needs an
/// oracle (it is not in the signed payload).
///
/// Returns `{ proceedsBelowFloorBps, proceedsUsd, floorUsd }` (4dp decimal
/// strings). `None` (→ field omitted → policy dormant, fail-open) when the floor
/// is unknown / zero / off-mainnet, or proceeds can't be valued — never warns on a
/// collection we cannot price.
async fn sign_order_proceeds_floor(
    params: &Value,
    price_book: &dyn PriceBook,
    floor: &dyn NftFloorOracle,
) -> Option<Value> {
    let chain = params.get("chain_id").and_then(Value::as_str)?;
    let offerer = params
        .get("offerer")
        .and_then(Value::as_str)?
        .to_lowercase();
    let offer = params.get("offer").and_then(Value::as_array)?;
    let consideration = params.get("consideration").and_then(Value::as_array)?;

    // floorEth = Σ floor(collection) over offered NFT legs (Alchemy reports ETH).
    let nft_kinds = ["erc721", "erc1155", "erc721_criteria", "erc1155_criteria"];
    let mut floor_eth = 0.0f64;
    for leg in offer {
        let kind = leg.get("kind").and_then(Value::as_str).unwrap_or_default();
        if nft_kinds.contains(&kind) {
            let token = leg.get("token").and_then(Value::as_str)?;
            floor_eth += floor.floor_eth(chain, &token.to_lowercase()).await?;
        }
    }
    if floor_eth <= 0.0 {
        return None; // no priceable NFT offered → dormant
    }
    // Convert the ETH floor to USD via the market-global WETH proxy price (the
    // same price `value_leg_usd` uses for native proceeds). No ETH price → dormant.
    let floor_usd = floor_eth * eth_price_usd(chain, price_book).await?;
    if floor_usd <= 0.0 {
        return None;
    }

    // proceedsUsd = Σ USD of consideration legs paid to the offerer (the signer).
    let mut proceeds_usd = 0.0f64;
    for leg in consideration {
        let recipient = leg
            .get("recipient")
            .and_then(Value::as_str)
            .map(str::to_lowercase);
        if recipient.as_deref() == Some(offerer.as_str()) {
            if let Some(usd) = value_leg_usd(chain, leg, price_book).await {
                proceeds_usd += usd;
            }
        }
    }

    let below = (1.0 - proceeds_usd / floor_usd).max(0.0) * 10_000.0;
    Some(json!({
        "proceedsBelowFloorBps": format!("{below:.4}"),
        "proceedsUsd": format!("{proceeds_usd:.4}"),
        "floorUsd": format!("{floor_usd:.4}"),
    }))
}

/// Server-side `marketplace.fulfill_overpay_vs_floor` (Seaport preset P3 — taker
/// payment safety): the BUY-side analog of below-floor. Reports how many times the
/// received NFT collection's USD floor the taker is PAYING in total consideration
/// (`overpayMultiple = paidUsd / floorUsd`). A fake mint / buy page can show a small
/// UI price while the calldata pays a huge consideration; this surfaces paying many
/// times the floor for what you receive.
///
/// Returns `{ overpayMultiple, paidUsd, floorUsd }` (4dp decimal strings). `None`
/// (→ field omitted → policy dormant, fail-open) when the floor is unknown / zero /
/// off-mainnet, or nothing priceable is paid — never warns on a collection we
/// cannot price. Floor is a WEAK upper anchor on the buy side (rare items sell far
/// above floor), so the policy pairs this with a generous threshold and warn-only.
async fn fulfill_overpay_vs_floor(
    params: &Value,
    price_book: &dyn PriceBook,
    floor: &dyn NftFloorOracle,
) -> Option<Value> {
    let chain = params.get("chain_id").and_then(Value::as_str)?;
    let offer = params.get("offer").and_then(Value::as_array)?;
    let consideration = params.get("consideration").and_then(Value::as_array)?;

    // floorEth = Σ floor(collection) over the RECEIVED NFT legs (Alchemy reports ETH).
    let nft_kinds = ["erc721", "erc1155", "erc721_criteria", "erc1155_criteria"];
    let mut floor_eth = 0.0f64;
    for leg in offer {
        let kind = leg.get("kind").and_then(Value::as_str).unwrap_or_default();
        if nft_kinds.contains(&kind) {
            let token = leg.get("token").and_then(Value::as_str)?;
            floor_eth += floor.floor_eth(chain, &token.to_lowercase()).await?;
        }
    }
    if floor_eth <= 0.0 {
        return None; // no priceable NFT received → dormant
    }
    // Convert the ETH floor to USD via the market-global WETH proxy price (the same
    // price `value_leg_usd` uses for native legs). No ETH price → dormant.
    let floor_usd = floor_eth * eth_price_usd(chain, price_book).await?;
    if floor_usd <= 0.0 {
        return None;
    }

    // paidUsd = Σ USD over ALL consideration legs (the taker pays the whole
    // consideration: seller proceeds + fees + royalties), unlike below-floor which
    // sums only the legs paid TO the offerer.
    let mut paid_usd = 0.0f64;
    for leg in consideration {
        if let Some(usd) = value_leg_usd(chain, leg, price_book).await {
            paid_usd += usd;
        }
    }
    if paid_usd <= 0.0 {
        return None; // nothing priceable paid → cannot judge overpay → dormant
    }

    let multiple = paid_usd / floor_usd;
    Some(json!({
        "overpayMultiple": format!("{multiple:.4}"),
        "paidUsd": format!("{paid_usd:.4}"),
        "floorUsd": format!("{floor_usd:.4}"),
    }))
}

/// Server-side `oracle.steth_peg_status_bps` (preset P4 — peg-aware stake safety):
/// directional stETH/ETH peg status in basis points, computed from the
/// market-global [`PriceBook`] ratio `price(stETH) / price(WETH)` — both USD, so
/// the USD unit cancels and the result is the stETH/ETH price ratio. WETH is the
/// ETH price proxy (native ETH is not a `(chain, address)` token).
///
/// Returns the shape P4 projects (`$.result.discountBps → stethDiscountBps`):
/// `{ discountBps, premiumBps, deviationBps, pegRatio }` as 4dp decimal strings
/// (`pegRatio` 6dp). Definitions (`r = stETH/ETH`):
///   - `discountBps  = max(1 − r, 0) × 10000`
///   - `premiumBps   = max(r − 1, 0) × 10000`
///   - `deviationBps = |1 − r|      × 10000`
///
/// `None` when either price is unknown / unparseable / non-positive → the optional
/// call's result is omitted (P4 stays dormant, fail-open), never a fabricated peg.
async fn oracle_steth_peg_status_bps(params: &Value, price_book: &dyn PriceBook) -> Option<Value> {
    /// Lido stETH (mainnet).
    const STETH: &str = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
    /// Canonical WETH (mainnet) — the ETH price proxy.
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

    let chain = params.get("chain_id").and_then(Value::as_str)?;
    let steth_p: f64 = price_book
        .price(chain, STETH)
        .await?
        .price_usd
        .parse()
        .ok()?;
    let weth_p: f64 = price_book
        .price(chain, WETH)
        .await?
        .price_usd
        .parse()
        .ok()?;
    if !(steth_p.is_finite() && weth_p.is_finite()) || weth_p <= 0.0 {
        return None;
    }
    let ratio = steth_p / weth_p;
    let discount_bps = (1.0 - ratio).max(0.0) * 10_000.0;
    let premium_bps = (ratio - 1.0).max(0.0) * 10_000.0;
    let deviation_bps = (1.0 - ratio).abs() * 10_000.0;
    Some(json!({
        "discountBps": format!("{discount_bps:.4}"),
        "premiumBps": format!("{premium_bps:.4}"),
        "deviationBps": format!("{deviation_bps:.4}"),
        "pegRatio": format!("{ratio:.6}"),
    }))
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

/// Server-side `address.sanctions`: screen the delegation operator (or any
/// counterparty) `address` via the injected on-chain [`SanctionsScreen`]. Params
/// are the manifest's `{ address, chain_id? }` — `address` is the lowered
/// operator/newOperator (0x-hex), `chain_id` the EIP-155 id (advisory; the v1
/// oracle is Ethereum mainnet, the `EigenLayer` chain, so a non-numeric CAIP-2
/// `chain_id` falls back to mainnet). Returns `{ "sanctioned": bool }` — the
/// shape both the delegate (`$.result.sanctioned → sanctioned`) and redelegate
/// (`$.result.sanctioned → newOperatorSanctioned`) manifests project. `None` when
/// the address is missing or the oracle could not answer → the result is omitted
/// (fail-open for the optional call), never a fabricated `false`.
async fn address_sanctions(params: &Value, sanctions: &dyn SanctionsScreen) -> Option<Value> {
    let address = params.get("address").and_then(asset_address)?;
    let chain_id = params.get("chain_id").and_then(Value::as_i64).unwrap_or(1);
    let flag = sanctions.is_sanctioned(chain_id, &address).await?;
    Some(json!({ "sanctioned": flag }))
}

/// ABI-encode `isSanctioned(address)` calldata: 4-byte selector `0xdf592f7d`
/// (`keccak256("isSanctioned(address)")[..4]`) + the 32-byte left-padded
/// lowercased address. `None` for a malformed (non-20-byte / non-hex) address.
pub(crate) fn sanctions_calldata(address: &str) -> Option<String> {
    let addr = address.trim_start_matches("0x").to_lowercase();
    if addr.len() != 40 || !addr.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("0xdf592f7d{addr:0>64}"))
}

/// Decode the `bool` returned by `isSanctioned`: a single 32-byte word, any
/// non-zero byte = `true`. `None` for an empty return (a revert surfaces as a
/// JSON-RPC error with no `result`, so an empty/odd word means "could not
/// screen" — never coerced to `false`).
pub(crate) fn decode_sanctioned(result_hex: &str) -> Option<bool> {
    let hex = result_hex.trim_start_matches("0x");
    if hex.is_empty() {
        return None;
    }
    Some(hex.bytes().any(|b| b != b'0'))
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
    use policy_transition::action::hyperliquid_core::{HlWithdrawAction, HyperliquidCoreAction};
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

    /// A test [`SanctionsScreen`] returning a fixed answer for ANY address.
    struct StubSanctions(Option<bool>);
    #[async_trait]
    impl SanctionsScreen for StubSanctions {
        async fn is_sanctioned(&self, _chain_id: i64, _address: &str) -> Option<bool> {
            self.0
        }
    }
    /// Default: the oracle cannot answer (`None`) → `address.sanctions` stays absent.
    fn no_sanctions() -> StubSanctions {
        StubSanctions(None)
    }

    /// A test [`NftFloorOracle`] returning a fixed floor (ETH) for ANY collection.
    struct StubFloor(Option<f64>);
    #[async_trait]
    impl NftFloorOracle for StubFloor {
        async fn floor_eth(&self, _chain: &str, _collection: &str) -> Option<f64> {
            self.0
        }
    }

    /// A priced book: ANY asset = $2000 @ 18 decimals (the ETH/WETH proxy price).
    fn priced_book() -> StubPriceBook {
        StubPriceBook(
            Some(PriceFact {
                price_usd: "2000".to_owned(),
                decimals: 18,
            }),
            Some(18),
        )
    }

    /// A standard Seaport listing params object: offer 1 NFT, consideration one
    /// native leg of `amount_hex` wei to `recipient`.
    fn floor_params(recipient: &str, amount_hex: &str) -> Value {
        serde_json::json!({
            "chain_id": "eip155:1",
            "offerer": "0x1111111111111111111111111111111111111111",
            "offer": [{
                "kind": "erc721",
                "token": "0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d",
                "startAmount": "0x1", "endAmount": "0x1"
            }],
            "consideration": [{
                "kind": "native",
                "startAmount": amount_hex, "endAmount": amount_hex,
                "recipient": recipient
            }]
        })
    }

    /// proceeds 0.01 ETH ($20) vs floor 5 ETH × $2000 = $10,000 ⇒ 99.8% below ⇒ 9980 bps.
    #[tokio::test]
    async fn proceeds_floor_below_threshold_reports_high_bps() {
        // 0.01 ETH = 1e16 wei = 0x2386f26fc10000.
        let params = floor_params(
            "0x1111111111111111111111111111111111111111",
            "0x2386f26fc10000",
        );
        let v = super::sign_order_proceeds_floor(&params, &priced_book(), &StubFloor(Some(5.0)))
            .await
            .expect("priced floor → Some");
        assert_eq!(v["proceedsBelowFloorBps"], serde_json::json!("9980.0000"));
    }

    /// Floor unknown (oracle returns None) ⇒ method returns None ⇒ policy dormant.
    #[tokio::test]
    async fn proceeds_floor_unknown_floor_is_none_dormant() {
        let params = floor_params(
            "0x1111111111111111111111111111111111111111",
            "0x2386f26fc10000",
        );
        assert!(
            super::sign_order_proceeds_floor(&params, &priced_book(), &StubFloor(None))
                .await
                .is_none()
        );
    }

    /// The only consideration leg pays a THIRD party (not the offerer) ⇒ proceeds
    /// $0 ⇒ 10000 bps (the aggregate proceeds filter on recipient == offerer).
    #[tokio::test]
    async fn proceeds_floor_ignores_legs_not_paid_to_offerer() {
        let params = floor_params(
            "0x000000000000000000000000000000000000a01c",
            "0x2386f26fc10000",
        );
        let v = super::sign_order_proceeds_floor(&params, &priced_book(), &StubFloor(Some(5.0)))
            .await
            .expect("priced floor → Some");
        assert_eq!(v["proceedsBelowFloorBps"], serde_json::json!("10000.0000"));
    }

    /// Floor known (5 ETH) but the ETH→USD price is unavailable (empty price book)
    /// ⇒ the floor cannot be converted ⇒ method returns None ⇒ policy dormant.
    #[tokio::test]
    async fn proceeds_floor_eth_price_unknown_is_none_dormant() {
        let params = floor_params(
            "0x1111111111111111111111111111111111111111",
            "0x2386f26fc10000",
        );
        assert!(
            super::sign_order_proceeds_floor(&params, &no_price_book(), &StubFloor(Some(5.0)))
                .await
                .is_none()
        );
    }

    // ── marketplace.fulfill_overpay_vs_floor (P3 taker payment safety) ──────

    /// Pay 60 ETH for a floor-1-ETH NFT ⇒ overpayMultiple 60.0 (the $2000 ETH price
    /// cancels: multiple = paidEth / floorEth). All consideration legs are summed
    /// regardless of recipient (the taker pays the whole consideration).
    #[tokio::test]
    async fn overpay_above_floor_reports_high_multiple() {
        // 60 ETH = 0x340aad21b3b700000.
        let params = floor_params(
            "0x1111111111111111111111111111111111111111",
            "0x340aad21b3b700000",
        );
        let v = super::fulfill_overpay_vs_floor(&params, &priced_book(), &StubFloor(Some(1.0)))
            .await
            .expect("priced floor → Some");
        assert_eq!(v["overpayMultiple"], serde_json::json!("60.0000"));
    }

    /// Floor unknown (oracle None) ⇒ method None ⇒ policy dormant.
    #[tokio::test]
    async fn overpay_unknown_floor_is_none_dormant() {
        let params = floor_params(
            "0x1111111111111111111111111111111111111111",
            "0x340aad21b3b700000",
        );
        assert!(
            super::fulfill_overpay_vs_floor(&params, &priced_book(), &StubFloor(None))
                .await
                .is_none()
        );
    }

    /// Floor known (1 ETH) but the ETH→USD price is unavailable ⇒ the floor cannot
    /// be converted ⇒ method None ⇒ policy dormant.
    #[tokio::test]
    async fn overpay_eth_price_unknown_is_none_dormant() {
        let params = floor_params(
            "0x1111111111111111111111111111111111111111",
            "0x340aad21b3b700000",
        );
        assert!(
            super::fulfill_overpay_vs_floor(&params, &no_price_book(), &StubFloor(Some(1.0)))
                .await
                .is_none()
        );
    }

    /// A test [`PriceBook`] returning DISTINCT prices for stETH vs WETH (keyed by
    /// address); any other asset → `None`. Lets `oracle.steth_peg_status_bps` see
    /// a real ratio (the shared `StubPriceBook` returns one price for everything).
    struct StethWethBook {
        steth: Option<&'static str>,
        weth: Option<&'static str>,
    }
    #[async_trait]
    impl PriceBook for StethWethBook {
        async fn price(&self, _chain: &str, address: &str) -> Option<PriceFact> {
            let p = match address.to_lowercase().as_str() {
                "0xae7ab96520de3a18e5e111b5eaab095312d7fe84" => self.steth?,
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2" => self.weth?,
                _ => return None,
            };
            Some(PriceFact {
                price_usd: p.to_owned(),
                decimals: 18,
            })
        }
        async fn decimals(&self, _chain: &str, _address: &str) -> Option<u8> {
            Some(18)
        }
    }

    #[tokio::test]
    async fn steth_peg_discount_projects_directional_bps() {
        // stETH $2970 / ETH $3000 = 0.99 ⇒ 100 bps discount, 0 premium.
        let book = StethWethBook {
            steth: Some("2970.0"),
            weth: Some("3000.0"),
        };
        let params = serde_json::json!({ "chain_id": "eip155:1" });
        let v = super::oracle_steth_peg_status_bps(&params, &book)
            .await
            .unwrap();
        assert_eq!(v["discountBps"], "100.0000");
        assert_eq!(v["premiumBps"], "0.0000");
        assert_eq!(v["deviationBps"], "100.0000");
    }

    #[tokio::test]
    async fn steth_peg_premium_has_zero_discount() {
        // stETH $3030 / ETH $3000 = 1.01 ⇒ 0 discount, 100 bps premium.
        let book = StethWethBook {
            steth: Some("3030.0"),
            weth: Some("3000.0"),
        };
        let params = serde_json::json!({ "chain_id": "eip155:1" });
        let v = super::oracle_steth_peg_status_bps(&params, &book)
            .await
            .unwrap();
        assert_eq!(v["discountBps"], "0.0000");
        assert_eq!(v["premiumBps"], "100.0000");
    }

    #[tokio::test]
    async fn steth_peg_missing_price_is_none_dormant() {
        // ETH price unknown ⇒ None ⇒ P4 stays dormant (fail-open), never fabricated.
        let book = StethWethBook {
            steth: Some("2970.0"),
            weth: None,
        };
        let params = serde_json::json!({ "chain_id": "eip155:1" });
        assert!(super::oracle_steth_peg_status_bps(&params, &book)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn address_sanctions_true_projects_sanctioned_true() {
        let params = serde_json::json!({ "address": "0xABCdef0000000000000000000000000000000001", "chain_id": 1 });
        let v = super::address_sanctions(&params, &StubSanctions(Some(true)))
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!({ "sanctioned": true }));
    }

    #[tokio::test]
    async fn address_sanctions_false_projects_sanctioned_false() {
        let params = serde_json::json!({ "address": "0x0000000000000000000000000000000000000002" });
        let v = super::address_sanctions(&params, &StubSanctions(Some(false)))
            .await
            .unwrap();
        assert_eq!(v, serde_json::json!({ "sanctioned": false }));
    }

    #[tokio::test]
    async fn address_sanctions_oracle_unavailable_is_none() {
        let params = serde_json::json!({ "address": "0x0000000000000000000000000000000000000003" });
        assert!(super::address_sanctions(&params, &no_sanctions())
            .await
            .is_none());
    }

    #[tokio::test]
    async fn address_sanctions_missing_address_is_none() {
        let params = serde_json::json!({ "chain_id": 1 });
        assert!(
            super::address_sanctions(&params, &StubSanctions(Some(true)))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn execute_call_specs_serves_address_sanctions() {
        let st = WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        let spec = CallSpec {
            manifest_id: "delegate-operator-sanctioned-warn".into(),
            call_id: "m::operator-sanctions".into(),
            method: "address.sanctions".into(),
            params: serde_json::json!({ "address": "0xabc0000000000000000000000000000000000001", "chain_id": 1 }),
            outputs: Vec::new(),
            optional: true,
        };
        let (results, _diag) = super::execute_call_specs(
            &st,
            &[spec],
            &no_price_book(),
            &StubSanctions(Some(true)),
            &NoFloorOracle,
        )
        .await;
        assert_eq!(
            results["m::operator-sanctions"],
            serde_json::json!({ "sanctioned": true })
        );
    }

    /// WIRING (dispatch): a `marketplace.sign_order_proceeds_floor` call-spec with
    /// the manifest-shaped lowered params reaches the method through the real
    /// `execute_call_specs` dispatch and lands the result keyed by `call_id`.
    #[tokio::test]
    async fn execute_call_specs_serves_sign_order_proceeds_floor() {
        let st = WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        let spec = CallSpec {
            manifest_id: "listing-proceeds-below-floor-warn".into(),
            call_id: "floor::1".into(),
            method: "marketplace.sign_order_proceeds_floor".into(),
            params: floor_params(
                "0x1111111111111111111111111111111111111111",
                "0x2386f26fc10000",
            ),
            outputs: Vec::new(),
            optional: true,
        };
        let (results, _diag) = super::execute_call_specs(
            &st,
            &[spec],
            &priced_book(),
            &StubSanctions(None),
            &StubFloor(Some(5.0)),
        )
        .await;
        assert_eq!(
            results["floor::1"]["proceedsBelowFloorBps"],
            serde_json::json!("9980.0000")
        );
    }

    /// WIRING (dispatch): the fulfill overpay call-spec reaches the method via the
    /// real `execute_call_specs` dispatch and lands keyed by `call_id`.
    #[tokio::test]
    async fn execute_call_specs_serves_fulfill_overpay_vs_floor() {
        let st = WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        let spec = CallSpec {
            manifest_id: "fulfill-overpay-vs-floor-warn".into(),
            call_id: "overpay::1".into(),
            method: "marketplace.fulfill_overpay_vs_floor".into(),
            params: floor_params(
                "0x1111111111111111111111111111111111111111",
                "0x340aad21b3b700000",
            ),
            outputs: Vec::new(),
            optional: true,
        };
        let (results, _diag) = super::execute_call_specs(
            &st,
            &[spec],
            &priced_book(),
            &StubSanctions(None),
            &StubFloor(Some(1.0)),
        )
        .await;
        assert_eq!(
            results["overpay::1"]["overpayMultiple"],
            serde_json::json!("60.0000")
        );
    }

    /// WIRING (full public entry): an `EvaluateRequest` carrying the floor
    /// call-spec flows through `evaluate` → `execute_call_specs` → dispatch →
    /// method, and the computed `proceedsBelowFloorBps` appears in the response
    /// results map (the same map the SW replays into `context.custom.*`).
    #[tokio::test]
    async fn evaluate_serves_sign_order_proceeds_floor_into_results() {
        let store = InMemoryWalletStore::new();
        let mut req = empty_envelope_request();
        req.call_specs.push(CallSpec {
            manifest_id: "listing-proceeds-below-floor-warn".into(),
            call_id: "floor::1".into(),
            method: "marketplace.sign_order_proceeds_floor".into(),
            params: floor_params(
                "0x1111111111111111111111111111111111111111",
                "0x2386f26fc10000",
            ),
            outputs: Vec::new(),
            optional: true,
        });
        let resp = evaluate(
            &store,
            &priced_book(),
            &no_sanctions(),
            &StubFloor(Some(5.0)),
            req,
        )
        .await
        .unwrap();
        assert_eq!(
            resp.policy_request.results["floor::1"]["proceedsBelowFloorBps"],
            serde_json::json!("9980.0000")
        );
    }

    #[test]
    fn sanctions_calldata_pads_address_and_prefixes_selector() {
        let cd = super::sanctions_calldata("0xABCdef0000000000000000000000000000000001").unwrap();
        assert_eq!(
            cd,
            "0xdf592f7d000000000000000000000000abcdef0000000000000000000000000000000001"
        );
        assert_eq!(cd.len(), 2 + 8 + 64);
    }

    #[test]
    fn sanctions_calldata_rejects_malformed() {
        assert!(super::sanctions_calldata("0x1234").is_none()); // wrong length
        assert!(super::sanctions_calldata("0xzz000000000000000000000000000000000000").is_none());
        // non-hex
    }

    #[test]
    fn decode_sanctioned_true_false_and_empty() {
        assert_eq!(
            super::decode_sanctioned(
                "0x0000000000000000000000000000000000000000000000000000000000000001"
            ),
            Some(true)
        );
        assert_eq!(
            super::decode_sanctioned(
                "0x0000000000000000000000000000000000000000000000000000000000000000"
            ),
            Some(false)
        );
        assert_eq!(super::decode_sanctioned("0x"), None);
        assert_eq!(super::decode_sanctioned(""), None);
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

    fn hyperliquid_withdraw_action() -> Action {
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
            body: ActionBody::HyperliquidCore(HyperliquidCoreAction::Withdraw(HlWithdrawAction {
                destination: Address::from([0xde; 20]),
                amount: Decimal::new("50"),
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

        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            empty_envelope_request(),
        )
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
        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            empty_envelope_request(),
        )
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
            &no_sanctions(),
            &NoFloorOracle,
            request_with_envelope(hyperliquid_withdraw_action()),
        )
        .await
        .unwrap();

        assert_eq!(resp.policy_request.state_before, seeded);
        assert_eq!(resp.policy_request.deltas.len(), 1);
        assert_eq!(resp.policy_request.state_after.positions.len(), 1);
        match &resp.policy_request.state_after.positions[0].kind {
            PositionKind::HyperliquidAccount(account) => {
                assert_eq!(account.pending_outflow, Decimal::new("50"));
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

        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            req,
        )
        .await
        .unwrap();
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

        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            req,
        )
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

        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            req,
        )
        .await
        .unwrap();
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

        let resp = evaluate(&store, &price_book, &no_sanctions(), &NoFloorOracle, req)
            .await
            .unwrap();

        // 60_000 / 1e6 × 0.9996 = 0.059976 → "0.0600" (≥ 0.05 ⇒ a USD cap denies).
        assert_eq!(
            resp.policy_request.results["swap-usdc-usd-cap-deny::usd"],
            serde_json::json!({ "usd": "0.0600" })
        );
    }

    /// `bridge.value_loss_pct` computes the implied % value loss of a bridge from
    /// the two USD legs: input 100 USDC ($100) vs output 90 USDC ($90, on a
    /// different chain) → `(1 − 90/100) × 100 = 10.0000`. Both legs priced from the
    /// market-global book (empty wallet), exercising the cross-chain dual lookup.
    #[tokio::test]
    async fn bridge_value_loss_pct_computes_loss_pct() {
        let store = InMemoryWalletStore::new();
        let mut req = empty_envelope_request();
        req.call_specs.push(CallSpec {
            manifest_id: "bridge-output-value-loss-warn".into(),
            call_id: "bridge-output-value-loss-warn::bridge-value-loss".into(),
            method: "bridge.value_loss_pct".into(),
            params: serde_json::json!({
                "src_chain_id": "eip155:1",
                "src_asset": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "input_amount": "0x5f5e100",  // 100_000_000 = 100 USDC (6 decimals)
                "dst_chain_id": "eip155:8453",
                "dst_asset": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "output_amount": "0x55d4a80" // 90_000_000 = 90 USDC
            }),
            outputs: Vec::new(),
            optional: true,
        });
        // Same $1.0000/6-decimal fact for both legs (a same-asset USDC→USDC bridge).
        let price_book = StubPriceBook(
            Some(PriceFact {
                price_usd: "1.0000".into(),
                decimals: 6,
            }),
            None,
        );

        let resp = evaluate(&store, &price_book, &no_sanctions(), &NoFloorOracle, req)
            .await
            .unwrap();
        assert_eq!(
            resp.policy_request.results["bridge-output-value-loss-warn::bridge-value-loss"],
            serde_json::json!({ "loss_pct": "10.0000" })
        );
    }

    /// An unpriced leg (no holding, no global price) yields no result — the policy
    /// stays dormant (fail-open), and a diagnostic records the miss. Mirrors
    /// `oracle_usd_value_skips_unpriced_asset`.
    #[tokio::test]
    async fn bridge_value_loss_pct_dormant_when_leg_unpriced() {
        let store = InMemoryWalletStore::new();
        let mut req = empty_envelope_request();
        req.call_specs.push(CallSpec {
            manifest_id: "bridge-output-value-loss-warn".into(),
            call_id: "bridge-output-value-loss-warn::bridge-value-loss".into(),
            method: "bridge.value_loss_pct".into(),
            params: serde_json::json!({
                "src_chain_id": "eip155:1",
                "src_asset": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "input_amount": "0x5f5e100",
                "dst_chain_id": "eip155:8453",
                "dst_asset": "0x1111111111111111111111111111111111111111",
                "output_amount": "0x55d4a80"
            }),
            outputs: Vec::new(),
            optional: true,
        });

        // No holdings, no price book → neither leg is priceable → None.
        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            req,
        )
        .await
        .unwrap();
        assert!(
            resp.policy_request.results.is_empty(),
            "unpriced leg → no result (dormant): {:?}",
            resp.policy_request.results
        );
        assert!(
            resp.diagnostics
                .iter()
                .any(|d| d.level == "warn" && d.message.contains("bridge.value_loss_pct")),
            "miss should surface a diagnostic"
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

        let resp = evaluate(&store, &price_book, &no_sanctions(), &NoFloorOracle, req)
            .await
            .unwrap();

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

        let resp = evaluate(&store, &price_book, &no_sanctions(), &NoFloorOracle, req)
            .await
            .unwrap();

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

        let (results, _diag) = execute_call_specs(
            &state,
            std::slice::from_ref(&spec),
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
        )
        .await;
        assert_eq!(
            results.get(&spec.call_id),
            Some(&serde_json::json!({ "capSumOverBalance": true }))
        );
    }

    /// Both new intent methods dispatch correctly through `execute_call_specs`
    /// over manifest-shaped params: near-duplicate (state membership) + validity
    /// horizon (pure params).
    #[tokio::test]
    async fn near_duplicate_and_validity_horizon_served() {
        use policy_state::pending::{
            AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
        };
        use policy_state::primitives::VenueRef;
        use policy_state::token::TokenRef;
        use policy_state::StateDelta;

        let usdc = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
        let weth = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
        let tref = |a: &str| TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str(a).unwrap(),
            },
        };
        let pending = PendingTx {
            id: "intent:one_inch_fusion:0xopen".into(),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef::new("one_inch_fusion"),
                sell: tref(usdc),
                buy: tref(weth),
                sell_max: U256::from(1u64),
                buy_min: U256::from(1u64),
                order_kind: OrderKind::Dutch,
            },
            commitment: AssetCommitment::PermitCap {
                token: tref(usdc),
                spender: Address::ZERO,
                max_out: U256::from(1u64),
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
        state.pending = vec![pending];

        let dup_spec = CallSpec {
            manifest_id: "ammlp-intent-duplicate-warn".into(),
            call_id: "ammlp-intent-duplicate-warn::near-duplicate-pending".into(),
            method: "intent.near_duplicate_pending".into(),
            params: serde_json::json!({
                "chain_id": "eip155:1",
                "owner": "0x0000000000000000000000000000000000000000",
                "action": {
                    "venue": { "name": "one_inch_fusion" },
                    "sell": { "key": { "standard": "erc20", "chain": "eip155:1", "address": usdc } },
                    "buy": { "key": { "standard": "erc20", "chain": "eip155:1", "address": weth } }
                }
            }),
            outputs: Vec::new(),
            optional: true,
        };
        let horizon_spec = CallSpec {
            manifest_id: "intent-validity-horizon-warn".into(),
            call_id: "intent-validity-horizon-warn::validity-horizon".into(),
            method: "intent.validity_horizon_sec".into(),
            params: serde_json::json!({ "valid_until": 5000, "now": 1000 }),
            outputs: Vec::new(),
            optional: true,
        };

        let (results, _diag) = execute_call_specs(
            &state,
            &[dup_spec.clone(), horizon_spec.clone()],
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
        )
        .await;
        assert_eq!(
            results.get(&dup_spec.call_id),
            Some(&serde_json::json!({ "duplicate": true }))
        );
        assert_eq!(
            results.get(&horizon_spec.call_id),
            Some(&serde_json::json!({ "horizonSec": 4000 }))
        );
    }

    /// `perp.equity_drawdown_bps` dispatches through `execute_call_specs` over
    /// a synced HL account: 5% below the day baseline / 8% below the HWM. The
    /// no-HL-account wallet is skipped with a diagnostic (field left unset →
    /// the optional call's policy stays dormant).
    #[tokio::test]
    async fn equity_drawdown_served_from_hl_account() {
        use policy_state::primitives::ProtocolRef;
        use policy_state::{Decimal, EquityAnchor, HlAccount, Position, PositionKind};

        let mut state = WalletState::new(sample_wallet_id());
        state.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_account_value_usd: Some(Decimal::new("920")),
                equity_baseline: Some(EquityAnchor {
                    value: Decimal::new("968.42"),
                    anchored_at: Time::from_unix(864_000),
                    trusted: true,
                }),
                equity_hwm: Some(Decimal::new("1000")),
                ..Default::default()
            }),
            primitives_synced_at: Time::from_unix(864_000),
            primitives_source: DataSource::UserSupplied,
        });

        let spec = CallSpec {
            manifest_id: "order-daily-loss-limit-warn".into(),
            call_id: "order-daily-loss-limit-warn::equity-drawdown".into(),
            method: "perp.equity_drawdown_bps".into(),
            params: serde_json::json!({ "chain_id": "hl-mainnet" }),
            outputs: Vec::new(),
            optional: true,
        };

        let (results, _diag) = execute_call_specs(
            &state,
            std::slice::from_ref(&spec),
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
        )
        .await;
        // day: (968.42-920)/968.42 ≈ 500 bps; peak: (1000-920)/1000 = 800 bps.
        assert_eq!(
            results.get(&spec.call_id),
            Some(&serde_json::json!({
                "dayDrawdownBps": 500,
                "peakDrawdownBps": 800,
                "baselineTrusted": true
            }))
        );

        // No synced HL account → served as absent + diagnostic, never a value.
        let empty = WalletState::new(sample_wallet_id());
        let (results, diag) = execute_call_specs(
            &empty,
            std::slice::from_ref(&spec),
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
        )
        .await;
        assert!(!results.contains_key(&spec.call_id));
        assert!(
            diag.iter()
                .any(|d| d.message.contains("perp.equity_drawdown_bps")),
            "diagnostic names the method: {diag:?}"
        );
    }

    /// WIRING (full public entry + identity): the request's `wallet_id` is
    /// what selects the state a `perp.*` method reads. Seed the MASTER's
    /// wallet with HL anchors; an `EvaluateRequest` carrying the master
    /// `wallet_id` serves the drawdown, while the same request keyed by a
    /// different wallet (the venue submitter sentinel, pre-SW-prereq behavior)
    /// loads an empty state and leaves the field unset. This is the server
    /// half of the extension's `walletAddress` override.
    #[tokio::test]
    async fn evaluate_loads_state_by_wallet_id_for_perp_methods() {
        use policy_state::primitives::ProtocolRef;
        use policy_state::{Decimal, EquityAnchor, HlAccount, Position, PositionKind};

        let master_id = WalletId::new(
            Address::from_str("0x676fa5b94067c2be14bc025df6c5c80dedf49a54").unwrap(),
            [ChainId::ethereum_mainnet()],
        );
        let mut master_state = WalletState::new(master_id.clone());
        master_state.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_account_value_usd: Some(Decimal::new("950")),
                equity_baseline: Some(EquityAnchor {
                    value: Decimal::new("1000"),
                    anchored_at: Time::from_unix(864_000),
                    trusted: true,
                }),
                equity_hwm: Some(Decimal::new("1000")),
                ..Default::default()
            }),
            primitives_synced_at: Time::from_unix(864_000),
            primitives_source: DataSource::UserSupplied,
        });
        let store = InMemoryWalletStore::new();
        store.seed(master_state);

        let spec = CallSpec {
            manifest_id: "order-daily-loss-limit-warn".into(),
            call_id: "order-daily-loss-limit-warn::equity-drawdown".into(),
            method: "perp.equity_drawdown_bps".into(),
            params: serde_json::json!({ "chain_id": "hl-mainnet" }),
            outputs: Vec::new(),
            optional: true,
        };

        // (a) wallet_id = master → the method reads the seeded HL account.
        let mut req = empty_envelope_request();
        req.wallet_id = master_id;
        req.call_specs.push(spec.clone());
        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            req,
        )
        .await
        .unwrap();
        assert_eq!(
            resp.policy_request.results.get(&spec.call_id),
            Some(&serde_json::json!({
                "dayDrawdownBps": 500,
                "peakDrawdownBps": 500,
                "baselineTrusted": true
            }))
        );

        // (b) wallet_id = a different wallet (e.g. the submitter sentinel) →
        // empty state loads → field unset (dormant), never a fabricated value.
        let mut req = empty_envelope_request(); // sample_wallet_id = 0x…a01c
        req.call_specs.push(spec.clone());
        let resp = evaluate(
            &store,
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
            req,
        )
        .await
        .unwrap();
        assert!(!resp.policy_request.results.contains_key(&spec.call_id));
        assert!(resp
            .diagnostics
            .iter()
            .any(|d| d.message.contains("perp.equity_drawdown_bps")));
    }

    /// `perp.session_fill_stats` dispatches through `execute_call_specs` over
    /// a synced fill window; an empty window is skipped with a diagnostic.
    #[tokio::test]
    async fn session_fill_stats_served_from_fill_window() {
        use policy_state::primitives::ProtocolRef;
        use policy_state::{Decimal, HlAccount, HlFillSummary, Position, PositionKind};

        let day_start_ms: u64 = 20_615 * 86_400 * 1000;
        let fill = |tid: u64, time: u64, pnl: &str| HlFillSummary {
            tid,
            time,
            coin: "BTC".to_owned(),
            closed_pnl: Decimal::new(pnl),
            px: Decimal::new("60000"),
            sz: Decimal::new("0.1"),
        };
        let mut state = WalletState::new(sample_wallet_id());
        state.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                fill_window: vec![
                    fill(3, day_start_ms + 3000, "-1.0"),
                    fill(2, day_start_ms + 2000, "-2.0"),
                    fill(1, day_start_ms + 1000, "5.0"),
                ],
                ..Default::default()
            }),
            primitives_synced_at: Time::from_unix(864_000),
            primitives_source: DataSource::UserSupplied,
        });

        let spec = CallSpec {
            manifest_id: "order-loss-streak-cooldown-warn".into(),
            call_id: "order-loss-streak-cooldown-warn::session-fill-stats".into(),
            method: "perp.session_fill_stats".into(),
            params: serde_json::json!({
                "chain_id": "hl-mainnet",
                "now": 20_615 * 86_400 + 3_600
            }),
            outputs: Vec::new(),
            optional: true,
        };

        let (results, _diag) = execute_call_specs(
            &state,
            std::slice::from_ref(&spec),
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
        )
        .await;
        assert_eq!(
            results.get(&spec.call_id),
            Some(&serde_json::json!({
                "lossStreak": 2,
                "lossesToday": 2,
                "tradesToday": 3,
                "realizedPnlTodayUsd": 2
            }))
        );

        // Empty window → absent + diagnostic (dormant, not zeros).
        let empty = WalletState::new(sample_wallet_id());
        let (results, diag) = execute_call_specs(
            &empty,
            std::slice::from_ref(&spec),
            &no_price_book(),
            &no_sanctions(),
            &NoFloorOracle,
        )
        .await;
        assert!(!results.contains_key(&spec.call_id));
        assert!(
            diag.iter()
                .any(|d| d.message.contains("perp.session_fill_stats")),
            "diagnostic names the method: {diag:?}"
        );
    }
}
