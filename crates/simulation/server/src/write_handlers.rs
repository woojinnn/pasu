//! Mutating endpoints — add wallets + trigger sync refresh.
//!
//! These are the counterpart to `read_handlers`. They take an
//! authenticated user, mutate that user's per-user SQLite, and (where
//! applicable) trigger the sync orchestrator to fetch live data over
//! RPC/oracles defined in `scopeball-sync.toml`.
//!
//! Sync completion fires a `wallet_synced` event on the per-user SSE
//! stream so a dashboard or extension subscribed to the activity feed
//! sees the refresh in real time.

use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
use simulation_state::primitives::{Address, ChainId, Duration, Price, Time};
use simulation_state::token::{Balance, TokenHolding, TokenKey, TokenKind};
use simulation_state::{WalletId, WalletState, WalletStore};
use simulation_sync::{discovery, CoinGeckoClient, DiscoveredToken, Orchestrator};

use crate::app::AppState;
use crate::auth::AuthUser;
use crate::events::types::{Event, WalletSync};

/// `POST /wallets` body.
#[derive(Debug, Deserialize)]
pub struct AddWalletReq {
    /// 0x address (case-insensitive — we store lower-cased internally).
    pub address: String,
    /// CAIP-2 chain ids (e.g. `["eip155:1", "eip155:42161"]`).
    ///
    /// Optional. When omitted or empty the server tracks the wallet
    /// against **every** chain the sync config (`scopeball-sync.toml`)
    /// has an RPC provider for. Multicall keeps the per-chain RPC
    /// cost flat (2 calls per chain regardless of token count), so
    /// "all chains" is cheap and matches the typical user mental
    /// model of an EVM address being shared across chains.
    #[serde(default)]
    pub chains: Vec<String>,
    /// Optional human-friendly label.
    #[serde(default)]
    pub label: Option<String>,
}

/// `POST /wallets` response.
#[derive(Debug, Serialize)]
pub struct AddWalletResp {
    pub wallet_id: WalletId,
    /// True when the auto-sync after add succeeded; false if it was
    /// skipped (no orchestrator) or errored (logged in `error`).
    pub synced: bool,
    /// How many TokenHolding rows were seeded for a brand-new wallet
    /// (0 for an already-tracked wallet, also 0 when discovery fails).
    #[serde(default)]
    pub discovered: usize,
    /// Non-fatal sync error message — caller can retry with /sync.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `POST /wallets` — start tracking a new wallet for the authenticated
/// user. Creates an empty `WalletState` row and immediately triggers a
/// best-effort sync so the dashboard sees real data within one tick.
pub async fn add_wallet(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<AddWalletReq>,
) -> Response {
    let id = match build_wallet_id(&req, &state) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };

    // Seed: if the wallet is brand-new, discover what it holds (native
    // gas + ERC-20s via Etherscan when configured) and pre-populate the
    // state so the orchestrator's price refresh has something to walk.
    // For already-known wallets this is a no-op — the existing holdings
    // stay put.
    let existing = match store.load(&id).await {
        Ok(s) => s,
        Err(e) => return internal(&format!("load: {e}")),
    };
    let is_new = existing == WalletState::new(id.clone());
    let discovered_count = if is_new {
        let mut seeded = existing.clone();
        let n = match seed_holdings(&mut seeded, &id, &state).await {
            Ok(n) => n,
            Err(e) => {
                // Discovery is best-effort. Save the empty state and let
                // the user POST /sync later or live with native-only.
                if let Err(save_err) = store.save(&existing).await {
                    return internal(&format!("save: {save_err}"));
                }
                return Json(AddWalletResp {
                    wallet_id: id,
                    synced: false,
                    discovered: 0,
                    error: Some(format!("discovery: {e}")),
                })
                .into_response();
            }
        };
        if let Err(e) = store.save(&seeded).await {
            return internal(&format!("save: {e}"));
        }
        n
    } else {
        0
    };

    // Best-effort sync. Failures here aren't fatal — the caller can
    // POST /sync later, and stale state is better than no wallet row.
    let (synced, sync_err) = match run_sync(&*store, &id, &state.orchestrator).await {
        Ok(()) => {
            state.event_bus.publish(
                user.user_id.clone(),
                Event::WalletSynced(WalletSync {
                    wallet: format!("{:#x}", id.address),
                    fields_updated: 0, // populated by /sync, not the seed path
                    fields_failed: 0,
                    synced_at: unix_now(),
                }),
            );
            (true, None)
        }
        Err(e) => (false, Some(e)),
    };

    Json(AddWalletResp {
        wallet_id: id,
        synced,
        discovered: discovered_count,
        error: sync_err,
    })
    .into_response()
}

/// Discover what tokens this wallet holds and seed empty
/// `TokenHolding` rows for each. The orchestrator's price refresh
/// fills in USD prices in a subsequent pass.
///
/// Order per chain:
///   1. Native gas balance via `eth_getBalance` (always, no key needed).
///   2. ERC-20 balances via Etherscan V2 when `etherscan` is set.
///
/// Returns the total count of newly-seeded holdings.
async fn seed_holdings(
    state_out: &mut WalletState,
    id: &WalletId,
    app: &AppState,
) -> Result<usize, String> {
    let router = app
        .orchestrator
        .router_arc()
        .ok_or_else(|| "orchestrator has no RpcRouter (sync config missing?)".to_string())?;

    let mut count = 0usize;
    for chain in &id.chains {
        // 1. Native gas balance.
        match discovery::fetch_native_balance(&router, chain, id.address).await {
            Ok(tok) => {
                tracing::info!(
                    chain = %chain,
                    address = %format!("{:#x}", id.address),
                    "seed: native balance ok"
                );
                state_out
                    .tokens
                    .insert(tok.key.clone(), discovered_to_holding(tok, chain));
                count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    chain = %chain,
                    address = %format!("{:#x}", id.address),
                    error = %e,
                    "seed: native balance failed"
                );
                return Err(format!("native {chain}: {e}"));
            }
        }

        // 2. ERC-20 — try Etherscan first when a key is configured
        //    (comprehensive). If it 4xx's (no Pro tier, rate-limit, banned IP,
        //    etc.) fall back to the hardcoded top-N catalog via Multicall so
        //    free-tier users still see major stablecoins + WETH/WBTC/UNI/LINK.
        //    Listing every ERC-20 a wallet holds is a Pro-only endpoint at
        //    Etherscan as of 2026 — the fallback is therefore the common path.
        let mut erc20s = Vec::new();
        let mut source: &'static str = "top-tokens";
        let mut etherscan_failed: Option<String> = None;

        if let Some(es) = app.etherscan.as_ref() {
            match es.list_erc20_balances(chain, id.address).await {
                Ok(v) => {
                    source = "etherscan";
                    erc20s = v;
                }
                Err(e) => {
                    etherscan_failed = Some(format!("{e}"));
                }
            }
        }
        // Fallback path — no etherscan, or etherscan errored.
        if etherscan_failed.is_some() || app.etherscan.is_none() {
            match discovery::discover_top_tokens(&router, chain, id.address).await {
                Ok(v) => {
                    erc20s = v;
                    source = if etherscan_failed.is_some() {
                        "top-tokens (etherscan-fallback)"
                    } else {
                        "top-tokens"
                    };
                    if let Some(es_err) = &etherscan_failed {
                        tracing::warn!(
                            chain = %chain,
                            etherscan_error = %es_err,
                            "seed: etherscan failed, falling back to top-tokens multicall"
                        );
                    }
                }
                Err(top_err) => {
                    tracing::warn!(
                        chain = %chain,
                        etherscan_error = %etherscan_failed.unwrap_or_else(|| "(no etherscan)".into()),
                        top_tokens_error = %top_err,
                        "seed: both erc20 paths failed — keeping wallet without tokens"
                    );
                    continue;
                }
            }
        }

        tracing::info!(
            chain = %chain,
            candidates = erc20s.len(),
            source,
            "seed: erc20 candidates fetched"
        );
        let mut nonzero = 0usize;
        for tok in erc20s {
            if tok.balance.is_zero() {
                continue;
            }
            state_out
                .tokens
                .insert(tok.key.clone(), discovered_to_holding(tok, chain));
            count += 1;
            nonzero += 1;
        }
        tracing::info!(
            chain = %chain,
            inserted = nonzero,
            "seed: erc20 non-zero balances inserted"
        );
    }
    tracing::info!(
        address = %format!("{:#x}", id.address),
        total = count,
        "seed: completed"
    );

    // Best-effort CoinGecko metadata backfill. Capped by `MAX_METADATA_LOOKUPS`
    // so a wallet with 100+ tokens doesn't burn 100 sequential HTTP calls
    // synchronously — the orchestrator can fill the rest on next sync.
    backfill_metadata(state_out, &app.coingecko).await;
    Ok(count)
}

/// Hit CoinGecko for every token in the seeded state that lacks
/// metadata. Caps at `MAX_METADATA_LOOKUPS` calls per request so the
/// caller doesn't wait too long on free-tier rate limits (~30 req/min).
async fn backfill_metadata(state: &mut WalletState, cg: &CoinGeckoClient) {
    const MAX_METADATA_LOOKUPS: usize = 12;

    let needs: Vec<(TokenKey, ChainId, Address)> = state
        .tokens
        .iter()
        .filter(|(_, h)| h.metadata.is_none())
        .filter_map(|(key, _)| match key {
            TokenKey::Erc20 { chain, address } => Some((key.clone(), chain.clone(), *address)),
            _ => None,
        })
        .take(MAX_METADATA_LOOKUPS)
        .collect();

    for (key, chain, address) in needs {
        if let Some(md) = cg.fetch_metadata(&chain, address).await {
            if let Some(h) = state.tokens.get_mut(&key) {
                h.metadata = Some(md);
            }
        }
    }
}

/// Convert a `DiscoveredToken` into a `TokenHolding` ready to land in
/// `WalletState.tokens`. Two source pointers are set:
///   - `primitives_source` → on-chain balance (`eth_getBalance` for
///     native, `balanceOf(address)` for ERC-20). The orchestrator's
///     refresh loop uses this to keep `balance` current.
///   - `price_usd: LiveField<Price>` → Chainlink USD feed when the
///     symbol maps to a known feed (USDC, ETH, WBTC, …). `synced_at`
///     starts at unix epoch 0 so the orchestrator picks it up on the
///     next refresh and fills in a real price. Unknown symbols get
///     `None` (the orchestrator skips them).
fn discovered_to_holding(tok: DiscoveredToken, chain: &ChainId) -> TokenHolding {
    let primitives_source = match &tok.key {
        TokenKey::Native { .. } => DataSource::OnchainView {
            chain: chain.clone(),
            contract: Address::ZERO,
            function: "eth_getBalance".into(),
            decoder_id: "eth_balance".into(),
        },
        TokenKey::Erc20 { address, .. } => DataSource::OnchainView {
            chain: chain.clone(),
            contract: *address,
            function: "balanceOf(address)".into(),
            decoder_id: "erc20_balance".into(),
        },
        _ => DataSource::UserSupplied,
    };
    let price_usd = chainlink_feed_for(&tok.symbol).map(|feed_id| {
        LiveField::new(
            Price::new("0"),
            DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: feed_id.to_string(),
            },
            Time::from_unix(0), // never synced — orchestrator picks up on first tick
        )
        .with_ttl(Duration::from_secs(60))
    });
    TokenHolding {
        key: tok.key,
        kind: TokenKind::Unknown,
        symbol: tok.symbol,
        decimals: tok.decimals,
        balance: Balance::fungible(tok.balance),
        committed: Balance::zero_fungible(),
        approved_to: None,
        price_usd,
        metadata: None,
        value_usd: None,
        last_synced_at: Time::from_unix(unix_now_u64()),
        primitives_source,
    }
}

/// Map a token symbol to its canonical Chainlink USD feed id. Returns
/// `None` for symbols without a wired feed — the orchestrator's
/// Chainlink registry decides per-chain availability separately.
fn chainlink_feed_for(symbol: &str) -> Option<&'static str> {
    // Wrapper tokens share the underlying asset's feed.
    match symbol.to_uppercase().as_str() {
        "ETH" | "WETH" | "STETH" | "WSTETH" => Some("ETH/USD"),
        "BTC" | "WBTC" => Some("WBTC/USD"),
        "USDC" | "USDC.E" | "USDBC" => Some("USDC/USD"),
        "USDT" => Some("USDT/USD"),
        "DAI" => Some("DAI/USD"),
        _ => None,
    }
}

/// `PATCH /wallets/:address` body.
#[allow(clippy::option_option)]
#[derive(Debug, Deserialize)]
pub struct PatchWalletReq {
    /// Display label. `None` (omitted) leaves the field untouched;
    /// explicit `null` clears it.
    #[serde(default, deserialize_with = "serde_helpers::deserialize_present")]
    pub label: Option<Option<String>>,
    /// Owned vs watch-only.
    #[serde(default)]
    pub is_owned: Option<bool>,
}

mod serde_helpers {
    use serde::{Deserialize, Deserializer};

    /// Distinguishes `{}` (field omitted → Option::None) from `{"label":
    /// null}` (field present-but-null → Option::Some(None)). PATCH
    /// semantics need that distinction.
    #[allow(clippy::option_option)]
    pub fn deserialize_present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}

/// `PATCH /wallets/:address` — update mutable display fields (label,
/// is_owned). Body is a partial JSON object; absent fields stay put.
pub async fn patch_wallet(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
    Json(req): Json<PatchWalletReq>,
) -> Response {
    let addr = match Address::from_str(&address) {
        Ok(a) => a,
        Err(e) => return bad_request(&format!("invalid address `{address}`: {e}")),
    };
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let addr_str = format!("{addr:#x}");
    let label = req.label;
    let is_owned = req.is_owned;
    let result = tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| {
            use simulation_db::repositories::wallets as wallets_repo;
            let Some(w) = wallets_repo::get_by_address(tx, &addr_str)? else {
                return Ok(None);
            };
            wallets_repo::update(tx, w.id, label.as_ref().map(|o| o.as_deref()), is_owned)?;
            Ok(Some(()))
        })
    })
    .await;
    match result {
        Ok(Ok(Some(()))) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(None)) => not_found("wallet not tracked for this user"),
        Ok(Err(e)) => internal(&format!("patch_wallet: {e}")),
        Err(e) => internal(&format!("join: {e}")),
    }
}

/// `DELETE /wallets/:address` — archive the wallet (soft delete).
/// Subsequent `GET /wallets` won't list it; the holdings rows stay so a
/// future un-archive could restore the snapshot.
pub async fn delete_wallet(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
) -> Response {
    let addr = match Address::from_str(&address) {
        Ok(a) => a,
        Err(e) => return bad_request(&format!("invalid address `{address}`: {e}")),
    };
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let addr_str = format!("{addr:#x}");
    let now = unix_now();
    let result = tokio::task::spawn_blocking(move || {
        pool.with_tx(|tx| {
            use simulation_db::repositories::wallets as wallets_repo;
            let Some(w) = wallets_repo::get_by_address(tx, &addr_str)? else {
                return Ok(false);
            };
            wallets_repo::archive(tx, w.id, now)
        })
    })
    .await;
    match result {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => not_found("wallet not tracked or already archived"),
        Ok(Err(e)) => internal(&format!("delete_wallet: {e}")),
        Err(e) => internal(&format!("join: {e}")),
    }
}

/// `POST /wallets/:address/sync` — force a refresh against live RPC/oracle
/// sources. Caller blocks until the orchestrator finishes (or errors).
pub async fn sync_wallet(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(address): Path<String>,
) -> Response {
    let addr = match Address::from_str(&address) {
        Ok(a) => a,
        Err(e) => return bad_request(&format!("invalid address `{address}`: {e}")),
    };

    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };

    // Find the wallet's chain set from the stored row.
    let known = match store.list_wallets().await {
        Ok(w) => w,
        Err(e) => return internal(&format!("list_wallets: {e}")),
    };
    let Some(id) = known.into_iter().find(|w| w.address == addr) else {
        return not_found("wallet not tracked for this user");
    };

    // Re-run discovery if (a) the wallet has no holdings at all, or
    // (b) it has only native gas balances and zero ERC-20s. The latter
    // means the original Etherscan discovery silently bailed out and
    // the user now expects "지금 동기화" to actually re-attempt.
    let pre = match store.load(&id).await {
        Ok(s) => s,
        Err(e) => return internal(&format!("pre-sync load: {e}")),
    };
    let has_any_erc20 = pre
        .tokens
        .keys()
        .any(|k| matches!(k, simulation_state::TokenKey::Erc20 { .. }));
    if pre.tokens.is_empty() || !has_any_erc20 {
        tracing::info!(
            address = %format!("{:#x}", id.address),
            had_total = pre.tokens.len(),
            had_erc20 = has_any_erc20,
            "sync: missing ERC-20 holdings — re-running discovery"
        );
        let mut seeded = pre.clone();
        match seed_holdings(&mut seeded, &id, &state).await {
            Ok(n) => {
                tracing::info!(seeded = n, "sync: discovery seeded N holdings");
                if let Err(e) = store.save(&seeded).await {
                    return internal(&format!("save after seed: {e}"));
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "sync: discovery failed — proceeding with previous wallet");
            }
        }
    }
    if let Err(e) = run_sync(&*store, &id, &state.orchestrator).await {
        return internal(&e);
    }
    // Re-load to count what changed — the orchestrator's RefreshReport
    // would be richer but we don't have it surfaced from `run_sync` yet.
    let _state = match store.load(&id).await {
        Ok(s) => s,
        Err(e) => return internal(&format!("post-sync load: {e}")),
    };
    state.event_bus.publish(
        user.user_id.clone(),
        Event::WalletSynced(WalletSync {
            wallet: format!("{:#x}", id.address),
            fields_updated: 0,
            fields_failed: 0,
            synced_at: unix_now(),
        }),
    );
    StatusCode::NO_CONTENT.into_response()
}

// ---------- internals ----------

/// Load the wallet state, perform authoritative venue/RPC sync, refresh stale
/// live fields, and persist the result. Execution reports are reconciled only
/// after an authoritative source updates the wallet snapshot; a local preflight
/// or extension report is never treated as final state by itself.
async fn run_sync(
    store: &dyn WalletStore,
    id: &WalletId,
    orchestrator: &Arc<Orchestrator>,
) -> Result<(), String> {
    let mut state = store
        .load(id)
        .await
        .map_err(|e| format!("load before sync: {e}"))?;
    let now = Time::from_unix(unix_now_u64());
    let mut authoritative_updated = false;

    let primitives = orchestrator
        .sync_primitives(&mut state, now)
        .await
        .map_err(|e| format!("orchestrator.sync_primitives: {e}"))?;
    authoritative_updated |= primitives.block_heights_updated
        + primitives.native_balances_updated
        + primitives.erc20_balances_updated
        + primitives.approvals_updated
        > 0;

    let hyperliquid = orchestrator
        .sync_hyperliquid_account(&mut state, now)
        .await
        .map_err(|e| format!("orchestrator.sync_hyperliquid_account: {e}"))?;
    authoritative_updated |= hyperliquid.account_updated;

    orchestrator
        .refresh(&mut state, now)
        .await
        .map_err(|e| format!("orchestrator.refresh: {e}"))?;
    store
        .save(&state)
        .await
        .map_err(|e| format!("save after sync: {e}"))?;

    if authoritative_updated {
        store
            .reconcile_reports(id, now)
            .await
            .map_err(|e| format!("reconcile reports: {e}"))?;
    }

    Ok(())
}

fn build_wallet_id(req: &AddWalletReq, state: &AppState) -> Result<WalletId, Response> {
    let address = Address::from_str(&req.address)
        .map_err(|e| bad_request(&format!("invalid address `{}`: {e}", req.address)))?;
    let chains: Vec<ChainId> = if req.chains.is_empty() {
        // Default — every chain the sync config has an RPC for. Better
        // UX than asking the user for CAIP-2 strings, and Multicall
        // keeps the per-chain cost flat.
        match state.orchestrator.router_arc() {
            Some(router) => router.chains().cloned().collect(),
            None => Vec::new(),
        }
    } else {
        req.chains.iter().cloned().map(ChainId::new).collect()
    };
    if chains.is_empty() {
        return Err(bad_request(
            "no chains configured on the server — set up scopeball-sync.toml or pass `chains` explicitly",
        ));
    }
    Ok(WalletId::new(address, chains))
}

fn bad_request(reason: &str) -> Response {
    (StatusCode::BAD_REQUEST, reason.to_owned()).into_response()
}

fn not_found(reason: &str) -> Response {
    (StatusCode::NOT_FOUND, reason.to_owned()).into_response()
}

fn internal(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}

fn unix_now() -> i64 {
    i64::try_from(unix_now_u64()).unwrap_or(0)
}

fn unix_now_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
