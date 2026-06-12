//! axum application wiring — router, shared state, and HTTP adapters.
//! `/openapi.yaml`) sit outside the auth layer; everything else sits behind
//! `require_auth` middleware so a missing / invalid JWT is rejected before
//! the handler runs.
//! State is shared as a single `AppState` carrying the per-user DB router
//! (`MultiUserStore`) plus the cross-user identity DB (`GlobalDb`).

use axum::extract::{FromRef, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::from_fn;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Extension, Json, Router};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use std::sync::Arc;
use std::time::Duration;

use policy_db::{GlobalDb, MultiUserStore};
use policy_sync::{CoinGeckoClient, EtherscanClient, Orchestrator};

use crate::auth::{require_auth, AuthUser};
use crate::config::ServerConfig;
use crate::coordination::DynCoordinator;
use crate::dashboard_handlers;
use crate::dto::EvaluateRequest;
use crate::events::{EventBus, EventPublisher};
use crate::handler::{
    evaluate, HandlerError, NftFloorOracle, PriceBook, PriceFact, SanctionsScreen,
};
use crate::market_handlers;
use crate::read_handlers;
use crate::write_handlers;

/// Shared, cheaply-cloneable application state handed to every handler.
/// `multi_user` resolves one `PostgreSQL`-backed wallet store per authenticated
/// user. `global_db` is the cross-user identity DB (email ↔ `user_id`).
#[derive(Clone)]
pub struct AppState {
    pub multi_user: MultiUserStore,
    pub global_db: GlobalDb,
    pub event_bus: EventBus,
    /// Fanout boundary for server-originated events. The local publisher writes
    /// into `event_bus`; cloud deployments can replace it with Redis pub/sub.
    pub publisher: Arc<dyn EventPublisher>,
    /// Sync orchestrator — wraps the per-protocol fetchers wired from
    /// `dambi-sync.toml`. Shared across handlers so we don't re-open
    /// HTTP connection pools on every request.
    pub orchestrator: Arc<Orchestrator>,
    /// Optional Etherscan V2 client — `None` when `ETHERSCAN_API_KEY`
    /// isn't set. `POST /wallets` uses it (when present) to discover
    /// every ERC-20 a wallet holds; absent it falls back to native-only.
    pub etherscan: Option<EtherscanClient>,
    /// `CoinGecko` metadata client — always present (free tier works
    /// keyless). `POST /wallets` calls it after discovery to backfill
    /// logo / website / description on newly-seen tokens. Lookups are
    /// best-effort; `CoinGecko` outages don't block wallet adds.
    pub coingecko: CoinGeckoClient,
    /// Cross-replica lock/idempotency boundary.
    pub coordinator: DynCoordinator,
    /// TTL used for user-scoped sync locks.
    pub sync_lock_ttl: Duration,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Orchestrator / CoinGeckoClient aren't Debug.
        f.debug_struct("AppState")
            .field("multi_user", &self.multi_user)
            .field("global_db", &self.global_db)
            .field("event_bus", &self.event_bus)
            .field("publisher", &"<EventPublisher>")
            .field("orchestrator", &"<Orchestrator>")
            .field(
                "etherscan",
                &self.etherscan.as_ref().map(|_| "<EtherscanClient>"),
            )
            .field("coingecko", &"<CoinGeckoClient>")
            .field("coordinator", &"<Coordinator>")
            .field("sync_lock_ttl", &self.sync_lock_ttl)
            .finish()
    }
}

// Sub-state extractors so handlers can ask for just the piece they need.
impl FromRef<AppState> for MultiUserStore {
    fn from_ref(s: &AppState) -> Self {
        s.multi_user.clone()
    }
}

impl FromRef<AppState> for GlobalDb {
    fn from_ref(s: &AppState) -> Self {
        s.global_db.clone()
    }
}

impl FromRef<AppState> for EventBus {
    fn from_ref(s: &AppState) -> Self {
        s.event_bus.clone()
    }
}

impl FromRef<AppState> for Arc<Orchestrator> {
    fn from_ref(s: &AppState) -> Self {
        s.orchestrator.clone()
    }
}

/// Cloneable shutdown signal injected as an axum `Extension` in `main`.
/// Long-lived handlers (SSE) end their streams when this flips to `true`
/// on SIGTERM, so graceful shutdown doesn't block on the 30s keepalive.
#[derive(Clone)]
pub struct ShutdownRx(pub tokio::sync::watch::Receiver<bool>);

/// Builds the service router.
///
/// Public (no auth):
/// - `GET  /health`                         — liveness probe.
/// - `GET  /docs`                           — Swagger UI page.
/// - `GET  /openapi.yaml`                   — `OpenAPI` 3.0 spec.
/// - `GET  /auth/google`                    — redirect to Google consent.
/// - `GET  /auth/google/callback`           — finish OAuth → JWT.
///
/// Authenticated (`Authorization: Bearer <jwt>` OR `?token=<jwt>` on
/// SSE — see `auth::middleware` for the resolution order):
/// - `GET  /auth/me`                        — current user (id + email).
/// - `POST /evaluate`                       — simulate action envelope(s).
/// - `GET  /wallets`                        — list user's wallets.
/// - `POST /wallets`                        — start tracking a new wallet.
/// - `PATCH/DELETE /wallets/:address`       — label/owned + archive.
/// - `POST /wallets/:address/sync`          — refresh via RPC/oracle.
/// - `POST /wallets/:address/permits`       — record a signed permit/permit2.
/// - `GET  /wallets/:address/state`         — full wallet state.
/// - `GET  /wallets/:address/holdings`      — token holdings.
/// - `GET  /wallets/:address/approvals`     — approval set.
/// - `GET  /wallets/:address/block-heights` — per-chain sync block.
/// - `GET  /transactions`                   — state-delta lifecycle log.
/// - `GET  /tokens`                         — token catalog + metadata.
/// - `GET  /events/stream`                  — SSE live event feed.
///
/// Policy installation, policy catalogs, verdict history, audit views, and
/// finding feeds are intentionally extension-local. The cloud API only stores
/// wallet state, token metadata, transactions, and sync lifecycle data.
///
/// CORS is allowlist-based in cloud mode. Local defaults still allow the
/// dashboard development origins configured in [`ServerConfig`].
pub fn build_router(state: AppState) -> Router {
    let config = ServerConfig::from_env();
    build_router_with_config(state, &config)
}

/// Builds the service router with explicit runtime configuration.
pub fn build_router_with_config(state: AppState, config: &ServerConfig) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(auth_me_handler))
        .route("/evaluate", post(evaluate_handler))
        .route(
            "/wallets",
            get(read_handlers::list_wallets).post(write_handlers::add_wallet),
        )
        .route(
            "/wallets/:address",
            axum::routing::patch(write_handlers::patch_wallet)
                .delete(write_handlers::delete_wallet),
        )
        .route("/wallets/:address/sync", post(write_handlers::sync_wallet))
        .route(
            "/wallets/:address/permits",
            post(write_handlers::ingest_permit),
        )
        .route("/wallets/:address/state", get(read_handlers::get_state))
        .route(
            "/wallets/:address/holdings",
            get(read_handlers::get_holdings),
        )
        .route(
            "/wallets/:address/approvals",
            get(read_handlers::get_approvals),
        )
        .route(
            "/wallets/:address/positions",
            get(read_handlers::get_positions),
        )
        .route("/wallets/:address/pending", get(read_handlers::get_pending))
        .route(
            "/wallets/:address/block-heights",
            get(read_handlers::get_block_heights),
        )
        .route("/transactions", get(read_handlers::list_transactions))
        .route("/tokens", get(read_handlers::list_tokens))
        .route("/dashboard/summary", get(dashboard_handlers::get_summary))
        .route("/events/stream", get(crate::events::sse_stream))
        // ---- Marketplace ---------------------------------------------------
        .route(
            "/market/listings",
            get(market_handlers::list_listings).post(market_handlers::create_listing),
        )
        .route("/market/listings/:slug", get(market_handlers::get_listing))
        .route(
            "/market/listings/id/:id",
            delete(market_handlers::delete_listing),
        )
        .route(
            "/market/listings/id/:id/versions",
            post(market_handlers::create_version),
        )
        .route(
            "/market/listings/id/:id/versions/:ver",
            get(market_handlers::get_version),
        )
        .route(
            "/market/listings/id/:id/install",
            post(market_handlers::create_install),
        )
        .route(
            "/market/listings/id/:id/reviews",
            get(market_handlers::list_reviews).post(market_handlers::create_review),
        )
        .route(
            "/market/listings/id/:id/report",
            post(market_handlers::create_listing_report),
        )
        .route(
            "/market/reviews/:id/report",
            post(market_handlers::create_review_report),
        )
        .route(
            "/market/listings/id/:id/watch",
            post(market_handlers::watch).delete(market_handlers::unwatch),
        )
        .route(
            "/market/reviews/:id/helpful",
            post(market_handlers::vote_helpful),
        )
        .route("/market/reports", get(market_handlers::list_reports))
        .route(
            "/market/reports/mine",
            get(market_handlers::list_my_reports),
        )
        .route(
            "/market/reports/:id",
            patch(market_handlers::update_report_status),
        )
        .route("/market/watches", get(market_handlers::list_watches))
        // Selector decode + revoke calldata builder + Cedar sequence sim
        // all moved to the dashboard (apps/web/src/tools/* + cedar/).
        // The server holds only wallet state and sync lifecycle data.
        .layer(from_fn(require_auth));

    let public = Router::new()
        .route("/health", get(health_handler))
        .route("/readyz", get(crate::readiness::readyz_handler))
        .route("/docs", get(crate::docs::docs_html))
        .route("/openapi.yaml", get(crate::docs::openapi_yaml))
        .route("/auth/google", get(crate::auth::start_google_login))
        .route("/auth/google/callback", get(crate::auth::google_callback))
        .route("/auth/refresh", post(crate::auth::refresh_token));

    public
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer(config))
        .with_state(state)
}

fn cors_layer(config: &ServerConfig) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .cors_allowed_origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_private_network(config.allow_private_network)
}

/// `GET /health` — liveness probe.
async fn health_handler() -> &'static str {
    "ok"
}

/// `GET /auth/me` — echo the authenticated user. Used by the dashboard
/// to validate a stored JWT on page load and render the profile chip.
async fn auth_me_handler(Extension(user): Extension<AuthUser>) -> Response {
    Json(serde_json::json!({
        "user_id": user.user_id,
        "email": user.email,
    }))
    .into_response()
}

/// Maps [`HandlerError::Reducer`] to `422 Unprocessable Entity` (the action is
/// invalid for the state) and [`HandlerError::Store`] to `500 Internal Server
/// Error` (persistence failed).
/// Adapts the global DB's market-wide price lookup to the handler's
/// [`PriceBook`] so `oracle.usd_value` can value a swap from the synced price of
/// ANY wallet holding that token — not just the requesting (possibly
/// unregistered) wallet. A lookup error degrades to "unknown price" (the call
/// then fail-closes upstream), never a 500.
struct DbPriceBook {
    global_db: GlobalDb,
}

#[async_trait::async_trait]
impl PriceBook for DbPriceBook {
    async fn price(&self, chain: &str, address: &str) -> Option<PriceFact> {
        match self.global_db.latest_token_price(chain, address).await {
            Ok(Some(fact)) => Some(PriceFact {
                price_usd: fact.price_usd,
                decimals: fact.decimals,
            }),
            Ok(None) => None,
            Err(err) => {
                tracing::warn!(%chain, %address, error = %err, "global price lookup failed");
                None
            }
        }
    }

    async fn decimals(&self, chain: &str, address: &str) -> Option<u8> {
        match self.global_db.latest_token_decimals(chain, address).await {
            Ok(decimals) => decimals,
            Err(err) => {
                tracing::warn!(%chain, %address, error = %err, "global decimals lookup failed");
                None
            }
        }
    }
}

/// Chainalysis sanctions-list address `0x40C5…c8fb` is the canonical on-chain
/// oracle on Ethereum mainnet (verified contract; `name()` = "Chainalysis
/// sanctions oracle"). v1 is mainnet-only — the `EigenLayer` delegation chain.
const CHAINALYSIS_ORACLE_MAINNET: &str = "0x40c57923924b5c5c5455c48d93317139addac8fb";

/// On-chain sanctions screen for [`SanctionsScreen`], backed by the Chainalysis
/// oracle (`isSanctioned(address)`). Reads the Ethereum-mainnet JSON-RPC URL from
/// `POLICY_SANCTIONS_RPC_URL`; when unset the screen returns `None` (screen
/// unavailable → the optional `address.sanctions` call fail-opens, so a
/// deployment without an RPC stays dormancy-safe). The `eth_call` is hard-bounded
/// (1.5 s) so a slow/dead RPC degrades to `None`, never blocking the verdict.
/// Honest limit: the oracle is bool-only (no list/label/timestamp) and lags OFAC
/// designations — a `true` is high-signal, a `false` is NOT an authoritative
/// "clean".
struct ChainalysisSanctionsOracle {
    client: reqwest::Client,
    rpc_url: Option<String>,
}

#[async_trait::async_trait]
impl SanctionsScreen for ChainalysisSanctionsOracle {
    async fn is_sanctioned(&self, chain_id: i64, address: &str) -> Option<bool> {
        if chain_id != 1 {
            return None; // v1: Ethereum mainnet only (the EigenLayer chain).
        }
        let rpc_url = self.rpc_url.as_deref()?;
        let data = crate::handler::sanctions_calldata(address)?;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{ "to": CHAINALYSIS_ORACLE_MAINNET, "data": data }, "latest"],
        });
        let resp = self
            .client
            .post(rpc_url)
            .json(&body)
            .timeout(std::time::Duration::from_millis(1500))
            .send()
            .await
            .ok()?
            .json::<serde_json::Value>()
            .await
            .ok()?;
        // A revert / error surfaces as a JSON-RPC `error` with no `result` → None.
        let result = resp.get("result")?.as_str()?;
        crate::handler::decode_sanctioned(result)
    }
}

/// Max age for a marketplace floor quote to be trusted. Alchemy background-
/// refreshes live marketplaces every ~15 min, so a quote older than this is from a
/// marketplace Alchemy no longer maintains (e.g. `LooksRare`, observed 17 months
/// stale) — dropping it stops a dead market's price from dragging the floor down (a
/// stale-LOW quote would undervalue the floor and miss a real dust drain) or
/// inflating it. Tunable.
const MAX_FLOOR_AGE: time::Duration = time::Duration::days(1);

/// Pick the floor (ETH) from an Alchemy `getFloorPrice` response: the LOWEST floor
/// among marketplaces whose quote is FRESH (`retrievedAt` within [`MAX_FLOOR_AGE`]
/// of `now`) AND valid (positive `floorPrice`, null `error`). Stale quotes are
/// dropped FIRST, so "lowest across marketplaces" is the cheapest CURRENT listing,
/// never the cheapest including months-old garbage. `None` when no marketplace has
/// a fresh, valid floor.
fn pick_fresh_floor_eth(body: &serde_json::Value, now: time::OffsetDateTime) -> Option<f64> {
    use time::format_description::well_known::Rfc3339;
    ["openSea", "looksRare"]
        .iter()
        .filter_map(|mkt| {
            let m = &body[*mkt];
            if m.get("error").is_some_and(|e| !e.is_null()) {
                return None; // marketplace reported an error
            }
            let price = m["floorPrice"].as_f64()?;
            if !(price.is_finite() && price > 0.0) {
                return None;
            }
            let retrieved =
                time::OffsetDateTime::parse(m["retrievedAt"].as_str()?, &Rfc3339).ok()?;
            if now - retrieved > MAX_FLOOR_AGE {
                return None; // stale quote
            }
            Some(price)
        })
        .reduce(f64::min)
}

/// NFT floor source for [`NftFloorOracle`], backed by Alchemy's `getFloorPrice`
/// NFT API. (Reservoir's hosted API was sunset 2025-10-15; Alchemy is its
/// recommended migration.) `getFloorPrice` reports the floor **in ETH** per
/// marketplace (`OpenSea` + `LooksRare`), **Ethereum mainnet only** — so v1 returns
/// `None` off `eip155:1`. We take the LOWEST floor among FRESH quotes
/// ([`pick_fresh_floor_eth`] drops stale ones first); the consuming method converts
/// ETH→USD via the WETH price. Reads
/// `ALCHEMY_NFT_API_URL` — the full NFT-API base incl. key, e.g.
/// `https://eth-mainnet.g.alchemy.com/nft/v3/<API_KEY>`; when unset the oracle
/// returns `None` (→ the optional `marketplace.sign_order_proceeds_floor` call
/// fail-opens, the below-floor policy stays dormant), never a fabricated floor.
/// Any network / non-200 / parse failure → `None`. The request is hard-bounded
/// (1.5 s) so a slow/dead API degrades to `None`, never blocking the verdict.
struct AlchemyFloorOracle {
    client: reqwest::Client,
    base_url: Option<String>,
}

#[async_trait::async_trait]
impl NftFloorOracle for AlchemyFloorOracle {
    async fn floor_eth(&self, chain: &str, collection: &str) -> Option<f64> {
        if chain != "eip155:1" {
            return None; // getFloorPrice is Ethereum-mainnet-only
        }
        let base = self.base_url.as_deref()?;
        let url = format!("{base}/getFloorPrice?contractAddress={collection}");
        let resp = self
            .client
            .get(&url)
            .header("accept", "application/json")
            .timeout(std::time::Duration::from_millis(1500))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body = resp.json::<serde_json::Value>().await.ok()?;
        // Response: `{ "openSea": { "floorPrice": <f64>, "priceCurrency": "ETH",
        // "retrievedAt": <rfc3339>, "error": null }, "looksRare": { … } }`. Drop
        // stale quotes, then take the lowest fresh floor.
        pick_fresh_floor_eth(&body, time::OffsetDateTime::now_utc())
    }
}

async fn evaluate_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<EvaluateRequest>,
) -> Response {
    tracing::debug!(
        user_id = %user.user_id,
        wallet_address = %format!("{:#x}", req.wallet_id.address),
        wallet_chains = ?req.wallet_id.chains,
        n_envelopes = req.envelopes.len(),
        n_call_specs = req.call_specs.len(),
        "evaluate request: wallet + enrichment call count"
    );
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("open user store: {e}"),
            )
                .into_response();
        }
    };
    let price_book = DbPriceBook {
        global_db: state.global_db.clone(),
    };
    let sanctions = ChainalysisSanctionsOracle {
        client: reqwest::Client::new(),
        rpc_url: std::env::var("POLICY_SANCTIONS_RPC_URL").ok(),
    };
    let floor = AlchemyFloorOracle {
        client: reqwest::Client::new(),
        base_url: std::env::var("ALCHEMY_NFT_API_URL").ok(),
    };
    match evaluate(&*store, &price_book, &sanctions, &floor, req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(err @ HandlerError::Reducer(_)) => {
            (StatusCode::UNPROCESSABLE_ENTITY, err.to_string()).into_response()
        }
        Err(err @ HandlerError::Store(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;

    fn at(ts: &str) -> OffsetDateTime {
        OffsetDateTime::parse(ts, &Rfc3339).unwrap()
    }

    /// Both quotes fresh → the LOWEST is the floor (cheapest current listing).
    #[test]
    fn both_fresh_takes_min() {
        let now = at("2026-06-10T05:00:00Z");
        let body = json!({
            "openSea":   { "floorPrice": 9.0, "retrievedAt": "2026-06-10T04:55:00Z", "error": null },
            "looksRare": { "floorPrice": 8.0, "retrievedAt": "2026-06-10T04:50:00Z", "error": null },
        });
        assert_eq!(super::pick_fresh_floor_eth(&body, now), Some(8.0));
    }

    /// A stale-HIGH quote is dropped; the fresh one wins (the real BAYC case:
    /// `OpenSea` 9.1 fresh vs `LooksRare` 69 stale 17 months).
    #[test]
    fn stale_high_dropped_fresh_wins() {
        let now = at("2026-06-10T05:00:00Z");
        let body = json!({
            "openSea":   { "floorPrice": 9.09999, "retrievedAt": "2026-06-10T04:51:00Z", "error": null },
            "looksRare": { "floorPrice": 69.0,    "retrievedAt": "2025-01-13T03:42:00Z", "error": null },
        });
        assert_eq!(super::pick_fresh_floor_eth(&body, now), Some(9.09999));
    }

    /// THE fix: a stale-LOW quote must NOT drag the floor down via min. The old
    /// raw-min(9.0, 3.0)=3.0 would undervalue the floor and MISS a real dust drain;
    /// dropping the stale source first yields the correct fresh 9.0.
    #[test]
    fn stale_low_dropped_not_min() {
        let now = at("2026-06-10T05:00:00Z");
        let body = json!({
            "openSea":   { "floorPrice": 9.0, "retrievedAt": "2026-06-10T04:55:00Z", "error": null },
            "looksRare": { "floorPrice": 3.0, "retrievedAt": "2024-06-10T00:00:00Z", "error": null },
        });
        assert_eq!(super::pick_fresh_floor_eth(&body, now), Some(9.0));
    }

    /// Both stale → no trustworthy floor → None (policy dormant, fail-open).
    #[test]
    fn both_stale_is_none() {
        let now = at("2026-06-10T05:00:00Z");
        let body = json!({
            "openSea":   { "floorPrice": 9.0, "retrievedAt": "2024-01-01T00:00:00Z", "error": null },
            "looksRare": { "floorPrice": 8.0, "retrievedAt": "2024-01-01T00:00:00Z", "error": null },
        });
        assert_eq!(super::pick_fresh_floor_eth(&body, now), None);
    }

    /// A marketplace that returned an error (non-null `error`) is skipped; the
    /// other fresh one wins.
    #[test]
    fn error_marketplace_skipped() {
        let now = at("2026-06-10T05:00:00Z");
        let body = json!({
            "openSea":   { "floorPrice": null, "retrievedAt": "2026-06-10T04:55:00Z", "error": "no floor" },
            "looksRare": { "floorPrice": 8.0,  "retrievedAt": "2026-06-10T04:50:00Z", "error": null },
        });
        assert_eq!(super::pick_fresh_floor_eth(&body, now), Some(8.0));
    }

    /// LIVE e2e (run with `--ignored` + `ALCHEMY_NFT_API_URL` set): the REAL
    /// `AlchemyFloorOracle` (reqwest fetch + `pick_fresh_floor_eth` stale filter)
    /// resolves BAYC's floor from live Alchemy to a sane positive ETH value —
    /// exercising the actual production code path end-to-end against the live API,
    /// not a stub. Ignored by default (network + key dependent).
    #[tokio::test]
    #[ignore = "hits live Alchemy; requires ALCHEMY_NFT_API_URL"]
    async fn live_alchemy_floor_bayc() {
        use crate::handler::NftFloorOracle;
        // Live + key-dependent: CI's `--ignored` run has no key, so SKIP (return
        // ok) instead of panicking when `ALCHEMY_NFT_API_URL` is unset. Runs the
        // real fetch only when a key is provided (local / a keyed CI run).
        let Ok(base) = std::env::var("ALCHEMY_NFT_API_URL") else {
            eprintln!("skipping live_alchemy_floor_bayc: ALCHEMY_NFT_API_URL not set");
            return;
        };
        let oracle = super::AlchemyFloorOracle {
            client: reqwest::Client::new(),
            base_url: Some(base),
        };
        let floor = oracle
            .floor_eth("eip155:1", "0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d")
            .await;
        assert!(
            matches!(floor, Some(p) if p.is_finite() && p > 0.0 && p < 100_000.0),
            "live BAYC floor should be a sane positive ETH value, got {floor:?}"
        );
    }
}
