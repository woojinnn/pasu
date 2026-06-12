//! `GET /dashboard/summary` — Home page + Monitoring L1 single-call payload.
//!
//! Aggregates across every wallet the authenticated user tracks:
//! - total portfolio USD (token holdings plus supported venue account assets)
//! - per-chain USD breakdown (`{chain, usd, pct}`)
//! - per-venue USD breakdown (`{venue, usd, pct}`)
//! - per-wallet summary (`{address, label, total_usd, unlimited_count, pending_count}`)
//! - workspace-level counters (`wallet_count`)
//!
//! Implementation deliberately walks the user's primitive wallet snapshots
//! one-by-one and reuses `WalletState::populate_computed_values()`. Aggregate
//! read models are derived on request instead of persisted in the database.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Serialize;

use policy_state::position::{HlAccount, HlSpotBalance, PositionKind};
use policy_state::primitives::ChainId;
use policy_state::{WalletId, WalletStore};

use crate::app::AppState;
use crate::auth::AuthUser;

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub wallet_count: usize,
    pub total_portfolio_usd: String,
    pub chain_breakdown: Vec<ChainShare>,
    pub venue_breakdown: Vec<VenueShare>,
    pub wallets: Vec<WalletSummary>,
}

#[derive(Debug, Serialize)]
pub struct ChainShare {
    pub chain: ChainId,
    pub usd: String,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct VenueShare {
    pub venue: String,
    pub usd: String,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct WalletSummary {
    pub id: i64,
    pub address: String,
    pub label: Option<String>,
    pub total_usd: String,
    pub unlimited_count: i64,
    pub pending_count: i64,
}

pub async fn get_summary(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };

    let wallet_rows = match store.list_wallet_metadata().await {
        Ok(rows) => rows,
        Err(e) => return internal(&format!("dashboard wallets: {e}")),
    };

    // Walk each wallet's state in parallel (small N), compute per-wallet
    // total + per-chain split.
    let mut total: f64 = 0.0;
    let mut by_chain: std::collections::BTreeMap<ChainId, f64> = std::collections::BTreeMap::new();
    let mut by_venue: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut wallet_summaries: Vec<WalletSummary> = Vec::with_capacity(wallet_rows.len());

    for (idx, w_row) in wallet_rows.into_iter().enumerate() {
        let wallet_id = match parse_addr(&w_row.address) {
            Some(addr) => WalletId::new(addr, w_row.chains.clone()),
            None => continue,
        };
        let Ok(mut s) = store.load(&wallet_id).await else {
            continue; // best-effort: skip on per-wallet load failure
        };
        let unlimited = count_unlimited_approvals(&s);
        let pending = i64::try_from(s.pending.len()).unwrap_or(i64::MAX);
        s.populate_computed_values();
        let token_total = s
            .portfolio_value_usd
            .as_ref()
            .and_then(|p| p.as_str().parse::<f64>().ok())
            .unwrap_or(0.0);
        let hl_total = hyperliquid_assets_usd(&s);
        let wallet_total = token_total + hl_total;
        total += wallet_total;
        if hl_total > 0.0 {
            *by_venue.entry("hyperliquid".to_owned()).or_default() += hl_total;
        }

        // Per-chain breakdown: sum value_usd grouped by token.chain.
        for (key, holding) in &s.tokens {
            let chain = key_chain(key).clone();
            let v = holding
                .value_usd
                .as_ref()
                .and_then(|p| p.as_str().parse::<f64>().ok())
                .unwrap_or(0.0);
            if v > 0.0 {
                *by_chain.entry(chain).or_default() += v;
            }
        }

        wallet_summaries.push(WalletSummary {
            id: i64::try_from(idx).unwrap_or(i64::MAX),
            address: w_row.address,
            label: w_row.label,
            total_usd: fmt6(wallet_total),
            unlimited_count: unlimited,
            pending_count: pending,
        });
    }

    let chain_breakdown: Vec<ChainShare> = by_chain
        .into_iter()
        .map(|(chain, usd)| ChainShare {
            chain,
            usd: fmt6(usd),
            pct: if total > 0.0 {
                (usd / total) * 100.0
            } else {
                0.0
            },
        })
        .collect();
    let venue_breakdown: Vec<VenueShare> = by_venue
        .into_iter()
        .map(|(venue, usd)| VenueShare {
            venue,
            usd: fmt6(usd),
            pct: if total > 0.0 {
                (usd / total) * 100.0
            } else {
                0.0
            },
        })
        .collect();

    Json(DashboardSummary {
        wallet_count: wallet_summaries.len(),
        total_portfolio_usd: fmt6(total),
        chain_breakdown,
        venue_breakdown,
        wallets: wallet_summaries,
    })
    .into_response()
}

fn fmt6(v: f64) -> String {
    format!("{v:.6}")
}

fn parse_addr(addr_lower: &str) -> Option<policy_state::primitives::Address> {
    use std::str::FromStr;
    policy_state::primitives::Address::from_str(addr_lower).ok()
}

fn key_chain(k: &policy_state::token::TokenKey) -> &ChainId {
    k.chain()
}

fn count_unlimited_approvals(state: &policy_state::WalletState) -> i64 {
    let erc20 = state
        .approvals
        .erc20
        .values()
        .flat_map(std::collections::BTreeMap::values)
        .filter(|spec| spec.is_unlimited)
        .count();
    let set_for_all: usize = state
        .approvals
        .set_for_all
        .values()
        .map(std::collections::BTreeSet::len)
        .sum();
    i64::try_from(erc20 + set_for_all).unwrap_or(i64::MAX)
}

fn hyperliquid_assets_usd(state: &policy_state::WalletState) -> f64 {
    state
        .positions
        .iter()
        .filter_map(|position| match &position.kind {
            PositionKind::HyperliquidAccount(account) => {
                Some(hyperliquid_account_assets_usd(account))
            }
            _ => None,
        })
        .sum()
}

fn hyperliquid_account_assets_usd(account: &HlAccount) -> f64 {
    let perp = account
        .perp_account_value_usd
        .as_ref()
        .or(account.perp_usdc.as_ref())
        .and_then(decimal_to_f64)
        .unwrap_or(0.0);
    let spot: f64 = account.spot_balances.iter().map(hyperliquid_spot_usd).sum();
    let vaults: f64 = account
        .vault_equities
        .iter()
        .filter_map(|vault| decimal_to_f64(&vault.equity))
        .sum();
    perp + spot + vaults
}

fn hyperliquid_spot_usd(balance: &HlSpotBalance) -> f64 {
    if is_usd_stable_spot(&balance.coin) {
        decimal_to_f64(&balance.total).unwrap_or(0.0)
    } else {
        0.0
    }
}

fn is_usd_stable_spot(coin: &str) -> bool {
    matches!(
        coin.to_ascii_uppercase().as_str(),
        "USDC" | "USDT" | "USDT0" | "USDE" | "USDH" | "USDXL"
    )
}

fn decimal_to_f64(decimal: &policy_state::Decimal) -> Option<f64> {
    decimal.as_str().parse::<f64>().ok()
}

fn internal(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{fmt6, hyperliquid_assets_usd};
    use policy_state::live_field::DataSource;
    use policy_state::position::{HlAccount, HlSpotBalance, HlVaultEquity, Position, PositionKind};
    use policy_state::primitives::{Address, ChainId, Decimal, Time};
    use policy_state::{ProtocolRef, WalletId, WalletState};

    fn state_with_hyperliquid_account(account: HlAccount) -> WalletState {
        let mut state = WalletState::new(WalletId::new(
            Address::from_str("0xd8da6bf26964af9d7eed9e03e53415d37aa96045").unwrap(),
            [ChainId::ethereum_mainnet()],
        ));
        state.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(account),
            primitives_synced_at: Time::from_unix(1_730_000_000),
            primitives_source: DataSource::VenueApi {
                endpoint: "https://api.hyperliquid.xyz/info".into(),
                parser_id: "hl_account".into(),
                auth: None,
            },
        });
        state
    }

    #[test]
    fn hyperliquid_assets_usd_counts_usdc_denominated_account_assets() {
        let state = state_with_hyperliquid_account(HlAccount {
            perp_usdc: Some(Decimal::new("10")),
            perp_account_value_usd: Some(Decimal::new("123.45")),
            pending_outflow: Decimal::zero(),
            positions: Vec::new(),
            open_orders: Vec::new(),
            spot_balances: vec![
                HlSpotBalance {
                    coin: "USDC".into(),
                    token: 0,
                    total: Decimal::new("50.25"),
                    hold: Decimal::zero(),
                    entry_ntl: Decimal::zero(),
                    available_after_maintenance: None,
                },
                HlSpotBalance {
                    coin: "HYPE".into(),
                    token: 150,
                    total: Decimal::new("3"),
                    hold: Decimal::zero(),
                    entry_ntl: Decimal::zero(),
                    available_after_maintenance: None,
                },
            ],
            staking: None,
            vault_equities: vec![HlVaultEquity {
                vault_address: Address::from_str("0x1111111111111111111111111111111111111111")
                    .unwrap(),
                equity: Decimal::new("200.75"),
                locked_until_timestamp: None,
            }],
            borrow_lend: None,
            leverage_settings: Vec::new(),
            agents: Vec::new(),
            ..HlAccount::default()
        });

        assert_eq!(fmt6(hyperliquid_assets_usd(&state)), "374.450000");
    }
}
