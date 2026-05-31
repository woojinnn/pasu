//! SQLite-backed [`WalletStore`].
//!
//! The scheduler needs a durable place to load and save authoritative snapshots.
//! This store uses the existing normalized `simulation-db` tables rather than
//! persisting reducer predictions. It intentionally reconciles execution reports
//! only after a sync tick has saved an authoritative wallet snapshot.

use async_trait::async_trait;

use simulation_db::repositories::{
    block_heights, execution_reports, holdings, positions, tokens, wallets,
};
use simulation_db::{DbError, Pool};
use simulation_state::store::StoreError;
use simulation_state::{
    Address, ChainId, DataSource, Position, PositionKind, ProtocolRef, Time, TokenKind, WalletId,
    WalletState, WalletStore,
};

/// SQLite-backed wallet state store for scheduler/server production wiring.
#[derive(Clone, Debug)]
pub struct SqliteWalletStore {
    pool: Pool,
}

impl SqliteWalletStore {
    /// Creates a wallet store over an already-migrated pool.
    #[must_use]
    pub const fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WalletStore for SqliteWalletStore {
    async fn list_wallets(&self) -> Result<Vec<WalletId>, StoreError> {
        let wallets = self.pool.with_tx(wallets::list_active)?;
        wallets
            .into_iter()
            .map(|w| {
                let address = parse_address(&w.address)?;
                Ok(WalletId::new(address, w.chains))
            })
            .collect()
    }

    async fn load(&self, id: &WalletId) -> Result<WalletState, StoreError> {
        let address = id.address.to_string().to_lowercase();
        let state = self.pool.with_tx(|tx| {
            let Some(wallet) = wallets::get_by_address(tx, &address)? else {
                return Ok(WalletState::new(id.clone()));
            };

            let wallet_id = WalletId::new(id.address, wallet.chains);
            let mut state = WalletState::new(wallet_id);

            for row in block_heights::list_for_wallet(tx, wallet.id)? {
                state.block_heights.insert(row.chain, row.height);
            }

            // `token_holdings` does not store `TokenKind`; until the catalog
            // layer owns semantic hydration, reload holdings as `Unknown`.
            // Primitive balances/prices remain intact for sync and display.
            for row in holdings::raw_list_for_wallet(tx, wallet.id)? {
                let holding = row.into_holding(TokenKind::Unknown)?;
                state.tokens.insert(holding.key.clone(), holding);
            }

            for row in positions::list_for_wallet(tx, wallet.id)? {
                state.positions.push(position_from_row(row)?);
            }

            Ok(state)
        })?;
        Ok(state)
    }

    async fn save(&self, state: &WalletState) -> Result<(), StoreError> {
        let created_at = unix_now_i64();
        self.pool.with_tx(|tx| {
            let address = state.wallet_id.address.to_string().to_lowercase();
            let wallet_row_id = if let Some(wallet) = wallets::get_by_address(tx, &address)? {
                wallets::replace_chains(tx, wallet.id, state.wallet_id.chains.iter().cloned())?;
                wallet.id
            } else {
                wallets::insert(
                    tx,
                    &wallets::WalletInsert {
                        address,
                        label: None,
                        is_owned: true,
                        created_at,
                        chains: state.wallet_id.chains.iter().cloned().collect(),
                    },
                )?
            };

            block_heights::delete_for_wallet(tx, wallet_row_id)?;
            for (chain, height) in &state.block_heights {
                block_heights::upsert(tx, wallet_row_id, chain, height)?;
            }

            holdings::delete_for_wallet(tx, wallet_row_id)?;
            for holding in state.tokens.values() {
                tokens::upsert(
                    tx,
                    &holding.key,
                    Some(&holding.symbol),
                    Some(holding.decimals),
                    created_at,
                )?;
                holdings::upsert(tx, wallet_row_id, holding)?;
            }

            positions::delete_for_wallet(tx, wallet_row_id)?;
            for position in &state.positions {
                positions::upsert(tx, &position_to_insert(wallet_row_id, position)?)?;
            }
            Ok(())
        })?;
        Ok(())
    }

    async fn reconcile_reports(&self, id: &WalletId, now: Time) -> Result<usize, StoreError> {
        let address = id.address.to_string().to_lowercase();
        let reconciled = self.pool.with_tx(|tx| {
            let Some(wallet) = wallets::get_by_address(tx, &address)? else {
                return Ok(0);
            };
            execution_reports::mark_reconciled_for_wallet(
                tx,
                wallet.id,
                i64::try_from(now.as_unix())
                    .map_err(|_| DbError::Invariant("reconcile time overflow".into()))?,
            )
        })?;
        Ok(reconciled)
    }
}

fn position_to_insert(
    wallet_id: i64,
    position: &Position,
) -> Result<positions::PositionInsert, DbError> {
    let primitives_synced_at = i64::try_from(position.primitives_synced_at.as_unix())
        .map_err(|_| DbError::Invariant("position synced_at overflow".into()))?;
    Ok(positions::PositionInsert {
        wallet_id,
        position_id: position.id.clone(),
        protocol: position.protocol.name.clone(),
        chain: position.chain.as_ref().map(ToString::to_string),
        kind: position_kind_name(&position.kind).to_owned(),
        market: position.protocol.market.clone(),
        summary: None,
        data: serde_json::to_value(&position.kind)?,
        primitives_synced_at,
        primitives_source: serde_json::to_value(&position.primitives_source)?,
    })
}

fn position_from_row(row: positions::PositionRow) -> Result<Position, DbError> {
    let kind = serde_json::from_str::<PositionKind>(&row.data_json)?;
    let primitives_source = serde_json::from_str::<DataSource>(&row.primitives_source_json)?;
    let synced_at = u64::try_from(row.primitives_synced_at)
        .map_err(|_| DbError::Invariant("position synced_at negative".into()))?;
    Ok(Position {
        id: row.position_id,
        protocol: ProtocolRef::new(row.protocol),
        chain: row.chain.map(ChainId::from),
        kind,
        primitives_synced_at: Time::from_unix(synced_at),
        primitives_source,
    })
}

fn position_kind_name(kind: &PositionKind) -> &'static str {
    match kind {
        PositionKind::LendingAccount(_) => "lending_account",
        PositionKind::PerpPosition(_) => "perp_position",
        PositionKind::AirdropClaim(_) => "airdrop_claim",
        PositionKind::LaunchpadAllocation(_) => "launchpad_allocation",
        PositionKind::VestingSchedule(_) => "vesting_schedule",
        PositionKind::HyperliquidAccount(_) => "hyperliquid_account",
    }
}

fn parse_address(address: &str) -> Result<Address, StoreError> {
    address
        .parse::<Address>()
        .map_err(|e| StoreError::Backend(format!("wallet address: {e}")))
}

fn unix_now_i64() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_db::repositories::execution_reports::{
        ExecutionReportInsert, OutcomeKind, ReportStage,
    };
    use simulation_db::run_migrations;
    use simulation_state::{BlockHeight, Decimal, HlAccount, ProtocolRef};

    fn sample_wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x362E7e9e630481631D7C804dfe50e24b53250925").unwrap(),
            [ChainId::ethereum_mainnet()],
        )
    }

    #[tokio::test]
    async fn sqlite_wallet_store_round_trips_positions_and_reconciles_reports() {
        let pool = Pool::open_in_memory();
        run_migrations(&pool).unwrap();
        let store = SqliteWalletStore::new(pool.clone());

        let mut state = WalletState::new(sample_wallet_id());
        state.block_heights.insert(
            ChainId::ethereum_mainnet(),
            BlockHeight {
                number: 25_000_000,
                time: 1_738_000_000,
            },
        );
        state.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_usdc: Some(Decimal::new("1128.21")),
                ..HlAccount::default()
            }),
            primitives_synced_at: Time::from_unix(1_738_000_000),
            primitives_source: DataSource::UserSupplied,
        });

        store.save(&state).await.unwrap();

        let wallet_row_id = pool
            .with_tx(|tx| {
                let wallet =
                    wallets::get_by_address(tx, "0x362e7e9e630481631d7c804dfe50e24b53250925")?
                        .unwrap();
                execution_reports::insert(
                    tx,
                    &ExecutionReportInsert {
                        wallet_id: Some(wallet.id),
                        evaluation_id: Some("eval-hl".into()),
                        action_index: Some(0),
                        stage: ReportStage::Venue,
                        outcome_kind: OutcomeKind::VenueAccepted,
                        chain: None,
                        tx_hash: None,
                        signature: None,
                        venue: Some("hyperliquid".into()),
                        venue_order_id: Some("42".into()),
                        client_order_id: None,
                        reason: None,
                        raw_json: serde_json::json!({"kind":"venue_accepted"}),
                        metadata_json: serde_json::json!({}),
                        created_at: 1_738_000_001,
                    },
                )?;
                Ok(wallet.id)
            })
            .unwrap();

        let loaded = store.load(&state.wallet_id).await.unwrap();
        assert_eq!(
            loaded
                .block_heights
                .get(&ChainId::ethereum_mainnet())
                .unwrap()
                .number,
            25_000_000
        );
        assert_eq!(loaded.positions.len(), 1);
        match &loaded.positions[0].kind {
            PositionKind::HyperliquidAccount(account) => {
                assert_eq!(
                    account.perp_usdc.as_ref().unwrap(),
                    &Decimal::new("1128.21")
                );
            }
            other => panic!("expected hyperliquid account, got {other:?}"),
        }

        let reconciled = store
            .reconcile_reports(&state.wallet_id, Time::from_unix(1_738_000_030))
            .await
            .unwrap();
        assert_eq!(reconciled, 1);
        pool.with_tx(|tx| {
            assert!(execution_reports::list_unreconciled_for_wallet(tx, wallet_row_id)?.is_empty());
            Ok(())
        })
        .unwrap();
    }
}
