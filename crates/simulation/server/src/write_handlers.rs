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

use simulation_state::live_field::DataSource;
use simulation_state::primitives::{Address, ChainId, Time};
use simulation_state::token::{Balance, TokenHolding, TokenKind};
use simulation_state::{WalletId, WalletState, WalletStore};
use simulation_sync::{discovery, DiscoveredToken, Orchestrator};

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
                state_out
                    .tokens
                    .insert(tok.key.clone(), discovered_to_holding(tok, chain));
                count += 1;
            }
            Err(e) => return Err(format!("native {chain}: {e}")),
        }

        // 2. ERC-20 — prefer Etherscan (comprehensive) when a key is
        //    configured; fall back to the hardcoded top-N catalog via
        //    Multicall so users without a key still see major
        //    stablecoins + WETH/WBTC/UNI/LINK/etc.
        let erc20s = if let Some(es) = app.etherscan.as_ref() {
            es.list_erc20_balances(chain, id.address)
                .await
                .map_err(|e| format!("etherscan {chain}: {e}"))?
        } else {
            discovery::discover_top_tokens(&router, chain, id.address)
                .await
                .map_err(|e| format!("top-tokens {chain}: {e}"))?
        };
        for tok in erc20s {
            if tok.balance.is_zero() {
                continue;
            }
            state_out
                .tokens
                .insert(tok.key.clone(), discovered_to_holding(tok, chain));
            count += 1;
        }
    }
    Ok(count)
}

/// Convert a `DiscoveredToken` into a `TokenHolding` ready to land in
/// `WalletState.tokens`. `primitives_source` points back at the
/// `eth_call balanceOf(...)` (or `eth_getBalance` for native) so the
/// orchestrator can refresh the balance on subsequent ticks.
fn discovered_to_holding(tok: DiscoveredToken, chain: &ChainId) -> TokenHolding {
    use simulation_state::token::TokenKey;
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
    TokenHolding {
        key: tok.key,
        kind: TokenKind::Unknown,
        symbol: tok.symbol,
        decimals: tok.decimals,
        balance: Balance::fungible(tok.balance),
        committed: Balance::zero_fungible(),
        approved_to: None,
        price_usd: None,
        last_synced_at: Time::from_unix(unix_now_u64()),
        primitives_source,
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

/// Load the wallet state, refresh it through the orchestrator (which
/// hits RPC/oracle endpoints), and save it back.
async fn run_sync(
    store: &dyn WalletStore,
    id: &WalletId,
    orchestrator: &Arc<Orchestrator>,
) -> Result<(), String> {
    let mut state = store
        .load(id)
        .await
        .map_err(|e| format!("load before sync: {e}"))?;
    orchestrator
        .refresh(&mut state, Time::from_unix(unix_now_u64()))
        .await
        .map_err(|e| format!("orchestrator.refresh: {e}"))?;
    store
        .save(&state)
        .await
        .map_err(|e| format!("save after sync: {e}"))
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
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
